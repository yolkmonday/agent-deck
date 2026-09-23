//! Guarded process verification and signalling for the recovery actions.
//!
//! Between the snapshot that flagged a session as stuck and the click that acts
//! on it, the pid can die and be reused by something unrelated. Every signal
//! therefore re-reads the pid's command line first and refuses unless it is
//! still a known agent. A refusal is a normal outcome, not an error to work
//! around.

use collector::process::ProcessTable;
use std::time::{Duration, Instant};

/// How often the exit poll asks the process table whether the pid is still
/// there. `ps` is a process of its own, so this stays well above the cost of a
/// single lookup.
const POLL_MS: u64 = 25;

/// The three binaries the dashboard is willing to signal. The macOS GUI app
/// `OpenCode` (capital O) is a different program and must never match, which
/// exact basename equality gives for free.
pub fn agent_kind(cmd: &str) -> Option<&'static str> {
    let exe = cmd.split_whitespace().next()?;
    match exe.rsplit('/').next()? {
        "claude" => Some("claude"),
        "opencode" => Some("opencode"),
        "codex" => Some("codex"),
        _ => None,
    }
}

/// The agent kind behind a pid right now, or the Indonesian reason it must not
/// be touched. Both failures are refusals the UI shows as information.
pub fn verify(pid: u32, procs: &dyn ProcessTable) -> Result<String, String> {
    if !procs.is_alive(pid) {
        return Err("proses sudah tidak ada".into());
    }
    let command = procs
        .list()
        .into_iter()
        .find(|p| p.pid == pid)
        .map(|p| p.command);
    match command.as_deref().and_then(agent_kind) {
        Some(kind) => Ok(kind.to_string()),
        None => Err("pid sudah bukan proses agent".into()),
    }
}

/// SIGTERM first, then SIGKILL only if the process is still there after
/// `wait_ms`, so an agent gets its chance to flush a transcript. Returns whether
/// SIGKILL was needed.
///
/// Pid 0, pid 1 and this dashboard's own pid are refused outright. A negative
/// pid cannot reach here at all: the parameter is unsigned.
#[cfg(unix)]
pub fn terminate(pid: u32, procs: &dyn ProcessTable, wait_ms: u64) -> Result<bool, String> {
    if pid <= 1 || pid == std::process::id() {
        return Err("pid tidak boleh dimatikan".into());
    }
    if !signal(pid, libc::SIGTERM)? {
        return Ok(false);
    }
    if wait_for_exit(pid, procs, wait_ms) {
        return Ok(false);
    }
    signal(pid, libc::SIGKILL)
}

/// SIGTERM/SIGKILL are unix signals; this dashboard has no Windows recovery
/// path yet, so the action simply refuses.
#[cfg(not(unix))]
pub fn terminate(_pid: u32, _procs: &dyn ProcessTable, _wait_ms: u64) -> Result<bool, String> {
    Err("menghentikan proses tidak didukung di platform ini".into())
}

/// `Ok(false)` means the pid was already gone, which is the outcome the caller
/// wanted anyway.
#[cfg(unix)]
fn signal(pid: u32, sig: libc::c_int) -> Result<bool, String> {
    let rc = unsafe { libc::kill(pid as libc::pid_t, sig) };
    if rc == 0 {
        return Ok(true);
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(false);
    }
    Err(format!("gagal mengirim sinyal ke pid {pid}: {err}"))
}

fn wait_for_exit(pid: u32, procs: &dyn ProcessTable, wait_ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(wait_ms);
    loop {
        if !procs.is_alive(pid) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(POLL_MS));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use collector::process::{FakeProcessTable, ProcInfo, SystemProcessTable};
    use std::io::BufRead;
    use std::process::{ChildStdin, ChildStdout, Command, Stdio};

    fn table(alive: &[u32], procs: &[(u32, &str)]) -> FakeProcessTable {
        FakeProcessTable {
            alive: alive.iter().copied().collect(),
            procs: procs
                .iter()
                .map(|(pid, command)| ProcInfo {
                    pid: *pid,
                    command: (*command).to_string(),
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Spawns a real child and reaps it in the background, so the pid never
    /// lingers as a zombie that `ps` would keep reporting as alive. The stdin
    /// handle comes back to the caller because dropping it would let `cat` see
    /// EOF and exit before the test ever signals it; the stdout pipe comes back
    /// so a test can wait for a readiness marker instead of sleeping.
    fn spawn(
        cmd: &str,
        args: &[&str],
    ) -> (u32, ChildStdin, ChildStdout, std::thread::JoinHandle<()>) {
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let reaper = std::thread::spawn(move || {
            let _ = child.wait();
        });
        (pid, stdin, stdout, reaper)
    }

    /// Polls rather than sleeping a fixed amount: a zombie that has not been
    /// reaped yet still answers `ps`.
    fn gone(procs: &SystemProcessTable, pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if !procs.is_alive(pid) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(POLL_MS));
        }
    }

    #[test]
    fn agent_kind_matches_the_three_binaries() {
        assert_eq!(agent_kind("/x/bin/claude"), Some("claude"));
        assert_eq!(agent_kind("opencode run task -m kn/x"), Some("opencode"));
        assert_eq!(agent_kind("/y/codex"), Some("codex"));
        assert_eq!(
            agent_kind("/Applications/OpenCode.app/Contents/MacOS/OpenCode"),
            None
        );
        assert_eq!(agent_kind("/usr/bin/vim"), None);
    }

    #[test]
    fn verify_rejects_a_dead_pid() {
        let procs = table(&[], &[]);
        assert_eq!(verify(4242, &procs), Err("proses sudah tidak ada".to_string()));
    }

    #[test]
    fn verify_rejects_a_recycled_pid() {
        let procs = table(&[7], &[(7, "/usr/bin/vim")]);
        assert_eq!(
            verify(7, &procs),
            Err("pid sudah bukan proses agent".to_string())
        );
    }

    #[test]
    fn verify_accepts_a_live_agent() {
        let procs = table(&[9], &[(9, "/Users/yolk/.local/bin/claude --resume")]);
        assert_eq!(verify(9, &procs), Ok("claude".to_string()));
    }

    #[test]
    fn terminate_refuses_protected_pids() {
        let procs = FakeProcessTable::default();
        for pid in [0, 1, std::process::id()] {
            assert_eq!(
                terminate(pid, &procs, 10),
                Err("pid tidak boleh dimatikan".to_string()),
                "pid {pid} was not refused"
            );
        }
    }

    #[test]
    fn terminate_ends_a_real_child() {
        let procs = SystemProcessTable;
        let (pid, _stdin, _stdout, reaper) = spawn("/bin/cat", &[]);
        assert!(procs.is_alive(pid), "cat was not running to begin with");

        let forced = terminate(pid, &procs, 5_000).unwrap();

        assert!(!forced, "SIGKILL was needed for a process that exits on SIGTERM");
        assert!(gone(&procs, pid), "cat survived SIGTERM");
        reaper.join().unwrap();
    }

    #[test]
    fn terminate_falls_back_to_sigkill() {
        let procs = SystemProcessTable;
        // The marker proves the trap is installed before anything is signalled:
        // signalling earlier would race the shell's own startup.
        let (pid, _stdin, stdout, reaper) =
            spawn("/bin/sh", &["-c", "trap \"\" TERM; echo ready; exec sleep 30"]);
        let mut ready = String::new();
        BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut ready).unwrap();
        assert_eq!(ready.trim(), "ready", "the trap was never installed");
        assert!(procs.is_alive(pid), "sh was not running to begin with");

        let forced = terminate(pid, &procs, 300).unwrap();

        assert!(forced, "a process that ignores SIGTERM must need SIGKILL");
        assert!(gone(&procs, pid), "sh survived SIGKILL");
        reaper.join().unwrap();
    }
}
