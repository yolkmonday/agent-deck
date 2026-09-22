use serde::Serialize;
use std::io::{Read, Write};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::time::{Duration, Instant};

/// Per-session output kept in memory so a re-mounted terminal view can replay it.
const SCROLLBACK_LIMIT: usize = 256 * 1024;
#[cfg(test)]
const EXIT_POLL_MS: u64 = 20;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TermProfile {
    pub id: String,
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub available: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TermSession {
    pub id: String,
    pub profile_id: String,
    pub label: String,
    pub cwd: String,
    pub command: String,
    pub started_at_ms: i64,
    pub alive: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TermDataEvent {
    pub id: String,
    pub chunk: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TermExitEvent {
    pub id: String,
    pub code: Option<i32>,
}

/// Carries the exit code from the waiter thread to any caller that polls the
/// registry. Liveness itself is tracked in the shared buffer, not here.
enum Signal {
    Exit(Option<i32>),
}

/// Output buffer shared between the reader thread and the registry.
///
/// The reader thread owns appending so a slow caller can never stall it: if the
/// buffer only grew when the app asked for it, a full channel would block the
/// reader, the PTY would fill, and the next `write` would deadlock.
#[derive(Default)]
struct Scrollback {
    bytes: Vec<u8>,
    alive: bool,
}

impl Scrollback {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() <= SCROLLBACK_LIMIT {
            return;
        }
        let mut cut = self.bytes.len() - SCROLLBACK_LIMIT;
        while cut < self.bytes.len() && (self.bytes[cut] & 0xC0) == 0x80 {
            cut += 1;
        }
        self.bytes.drain(..cut);
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

struct Session {
    info: TermSession,
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    buffer: Arc<Mutex<Scrollback>>,
    rx: Receiver<Signal>,
}

impl Session {
    /// Non-blocking: promotes a finished child to dead.
    fn poll(&mut self) {
        while let Ok(Signal::Exit(_code)) = self.rx.try_recv() {
            self.buffer.lock().unwrap().alive = false;
        }
        self.info.alive = self.buffer.lock().unwrap().alive;
    }

    fn kill_now(&mut self) {
        let _ = self.killer.kill();
        self.info.alive = false;
        self.buffer.lock().unwrap().alive = false;
    }
}

pub struct TerminalRegistry {
    profiles: Vec<TermProfile>,
    sessions: Vec<Session>,
    counter: u64,
}

fn builtin_profiles() -> Vec<TermProfile> {
    [
        ("claude", "Claude Code"),
        ("opencode", "opencode"),
        ("codex", "Codex"),
    ]
    .into_iter()
    .map(|(id, label)| TermProfile {
        id: id.to_string(),
        label: label.to_string(),
        program: id.to_string(),
        args: Vec::new(),
        available: on_path(id),
    })
    .collect()
}

/// Checks each `PATH` entry for an executable file. Deliberately does not shell
/// out to `which`: that would inherit the very user aliases this module exists
/// to avoid, and leaves the answer dependent on another program's output format.
pub fn on_path(program: &str) -> bool {
    if program.is_empty() {
        return false;
    }
    if program.contains('/') {
        return std::fs::metadata(program)
            .map(|m| m.is_file() && is_executable(&m))
            .unwrap_or(false);
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        std::fs::metadata(dir.join(program))
            .map(|m| m.is_file() && is_executable(&m))
            .unwrap_or(false)
    })
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    true
}

fn display_command(profile: &TermProfile) -> String {
    let mut parts = vec![profile.program.clone()];
    parts.extend(profile.args.iter().cloned());
    parts.join(" ")
}

impl Session {
    fn not_found(id: &str) -> anyhow::Error {
        anyhow::anyhow!("unknown terminal session: {id}")
    }
}

impl TerminalRegistry {
    fn slot<'a>(sessions: &'a mut [Session], id: &str) -> anyhow::Result<&'a mut Session> {
        sessions
            .iter_mut()
            .find(|s| s.info.id == id)
            .ok_or_else(|| Session::not_found(id))
    }

    pub fn new() -> TerminalRegistry {
        TerminalRegistry {
            profiles: builtin_profiles(),
            sessions: Vec::new(),
            counter: 0,
        }
    }

    /// Test-only: lets the suite spawn `/bin/echo` and `/bin/cat` instead of the
    /// agent binaries, which may not be installed on a build machine.
    #[cfg(test)]
    pub fn with_profiles(profiles: Vec<TermProfile>) -> TerminalRegistry {
        TerminalRegistry {
            profiles,
            sessions: Vec::new(),
            counter: 0,
        }
    }

    pub fn profiles() -> Vec<TermProfile> {
        builtin_profiles()
    }

    /// Spawns the profile's binary directly, with its explicit arguments and the
    /// user's inherited environment. No shell and no login profile is involved,
    /// so a shell alias (e.g. `codex -s danger-full-access`) is never applied.
    pub fn start<F, G>(
        &mut self,
        profile_id: &str,
        cwd: &str,
        on_data: F,
        on_exit: G,
    ) -> anyhow::Result<TermSession>
    where
        F: Fn(TermDataEvent) + Send + 'static,
        G: Fn(TermExitEvent) + Send + 'static,
    {
        let profile = self
            .profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown terminal profile: {profile_id}"))?;

        if !std::path::Path::new(cwd).is_dir() {
            anyhow::bail!("working directory does not exist: {cwd}");
        }
        if !on_path(&profile.program) {
            anyhow::bail!("program not found on PATH: {}", profile.program);
        }

        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system.openpty(portable_pty::PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = portable_pty::CommandBuilder::new(&profile.program);
        cmd.args(profile.args.iter());
        cmd.cwd(cwd);
        // When Agent Deck is itself launched from a Claude session, this marker is inherited and
        // the agent we spawn turns transcript saving OFF. The dashboard would then show that
        // session with no activity and no tokens, which is the opposite of the point. Sessions
        // started from here are the user's own, not a nested agent run.
        cmd.env_remove("CLAUDE_CODE_CHILD_SESSION");
        let child = pair.slave.spawn_command(cmd)?;
        let killer = child.clone_killer();
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        self.counter += 1;
        let id = format!("t{}", self.counter);
        let info = TermSession {
            id: id.clone(),
            profile_id: profile.id.clone(),
            label: profile.label.clone(),
            cwd: cwd.to_string(),
            command: display_command(&profile),
            started_at_ms: now_ms(),
            alive: true,
        };

        let (tx, rx) = std::sync::mpsc::channel::<Signal>();
        let buffer = Arc::new(Mutex::new(Scrollback {
            bytes: Vec::new(),
            alive: true,
        }));

        // The reader thread appends to the scrollback itself and stays
        // unblocked, so a caller that is slow to drain can never stall the PTY.
        std::thread::spawn({
            let id = id.clone();
            let buffer = buffer.clone();
            move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                            buffer.lock().unwrap().push(chunk.as_bytes());
                            on_data(TermDataEvent { id: id.clone(), chunk });
                        }
                        Err(_) => break,
                    }
                }
                buffer.lock().unwrap().alive = false;
            }
        });

        std::thread::spawn({
            let tx = tx;
            let id = id.clone();
            let buffer = buffer.clone();
            move || {
                let mut child = child;
                let status = child.wait();
                let code = status.ok().map(|s| s.exit_code() as i32);
                buffer.lock().unwrap().alive = false;
                let _ = tx.send(Signal::Exit(code));
                on_exit(TermExitEvent { id, code });
            }
        });

        self.sessions.push(Session {
            info: info.clone(),
            writer,
            master: pair.master,
            killer,
            buffer,
            rx,
        });

        Ok(info)
    }

    /// Also drains each session's exit signal, which is what promotes a finished
    /// child to `alive: false` without any polling timer of its own.
    pub fn list(&mut self) -> Vec<TermSession> {
        for s in self.sessions.iter_mut() {
            s.poll();
        }
        self.sessions.iter().map(|s| s.info.clone()).collect()
    }

    pub fn write(&mut self, id: &str, data: &str) -> anyhow::Result<()> {
        let session = Self::slot(&mut self.sessions, id)?;
        session.writer.write_all(data.as_bytes())?;
        session.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> anyhow::Result<()> {
        let session = self
            .sessions
            .iter()
            .find(|s| s.info.id == id)
            .ok_or_else(|| Session::not_found(id))?;
        session.master.resize(portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn kill(&mut self, id: &str) -> anyhow::Result<()> {
        let session = Self::slot(&mut self.sessions, id)?;
        session.kill_now();
        Ok(())
    }

    pub fn scrollback(&self, id: &str) -> String {
        match self.sessions.iter().find(|s| s.info.id == id) {
            Some(s) => s.buffer.lock().unwrap().text(),
            None => String::new(),
        }
    }

    pub fn kill_all(&mut self) {
        for s in self.sessions.iter_mut() {
            s.kill_now();
        }
    }
}

impl Default for TerminalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TerminalRegistry {
    fn drop(&mut self) {
        self.kill_all();
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn fixture(id: &str, program: &str, args: &[&str]) -> TermProfile {
        TermProfile {
            id: id.into(),
            label: id.into(),
            program: program.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            available: true,
        }
    }

    fn until<F: FnMut() -> bool>(deadline: Duration, mut check: F) -> bool {
        let start = Instant::now();
        loop {
            if check() {
                return true;
            }
            if start.elapsed() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(EXIT_POLL_MS));
        }
    }

    #[test]
    fn on_path_finds_a_real_binary_and_rejects_a_fake_one() {
        assert!(on_path("sh"));
        assert!(!on_path("definitely-not-a-real-binary-xyz"));
    }

    #[test]
    fn start_rejects_unknown_profile() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture("echo", "/bin/echo", &[])]);
        let err = reg
            .start("nope", "/tmp", |_| {}, |_| {})
            .unwrap_err()
            .to_string();
        assert!(err.contains("nope"), "unexpected error: {err}");
    }

    #[test]
    fn start_rejects_missing_cwd() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture("echo", "/bin/echo", &[])]);
        let err = reg
            .start("echo", "/no/such/dir/xyz", |_| {}, |_| {})
            .unwrap_err()
            .to_string();
        assert!(err.contains("/no/such/dir/xyz"), "unexpected error: {err}");
    }

    #[test]
    fn start_rejects_a_program_that_is_not_on_path() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture(
            "ghost",
            "definitely-not-a-real-binary-xyz",
            &[],
        )]);
        let err = reg
            .start("ghost", "/tmp", |_| {}, |_| {})
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("definitely-not-a-real-binary-xyz"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn start_runs_and_streams_output() {
        let mut reg =
            TerminalRegistry::with_profiles(vec![fixture("echo", "/bin/echo", &["hello"])]);
        let data = Arc::new(Mutex::new(String::new()));
        let exited = Arc::new(Mutex::new(false));
        let d2 = data.clone();
        let e2 = exited.clone();
        let session = reg
            .start(
                "echo",
                "/tmp",
                move |evt| d2.lock().unwrap().push_str(&evt.chunk),
                move |_| *e2.lock().unwrap() = true,
            )
            .unwrap();
        assert_eq!(session.command, "/bin/echo hello");
        assert!(
            until(Duration::from_secs(5), || data
                .lock()
                .unwrap()
                .contains("hello")),
            "no output arrived"
        );
        assert!(
            until(Duration::from_secs(5), || *exited.lock().unwrap()),
            "no exit arrived"
        );
    }

    #[test]
    fn write_reaches_the_process() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture("cat", "/bin/cat", &[])]);
        let data = Arc::new(Mutex::new(String::new()));
        let d2 = data.clone();
        let session = reg
            .start(
                "cat",
                "/tmp",
                move |evt| d2.lock().unwrap().push_str(&evt.chunk),
                |_| {},
            )
            .unwrap();
        reg.write(&session.id, "ping\n").unwrap();
        assert!(
            until(Duration::from_secs(5), || data.lock().unwrap().contains("ping")),
            "ping never came back"
        );
    }

    #[test]
    fn list_reflects_alive_then_dead() {
        // `/bin/cat` blocks on stdin, so it is genuinely alive when we look, and
        // `echo` may already be gone by then. Proving both states needs both.
        let mut reg = TerminalRegistry::with_profiles(vec![
            fixture("cat", "/bin/cat", &[]),
            fixture("echo", "/bin/echo", &["bye"]),
        ]);
        let live = reg.start("cat", "/tmp", |_| {}, |_| {}).unwrap();
        assert!(reg.list().iter().any(|s| s.id == live.id && s.alive));

        let gone = reg.start("echo", "/tmp", |_| {}, |_| {}).unwrap();
        assert!(
            until(Duration::from_secs(5), || reg
                .list()
                .iter()
                .any(|s| s.id == gone.id && !s.alive)),
            "session never reported dead"
        );
        assert!(reg.list().iter().any(|s| s.id == live.id && s.alive));

        reg.kill(&live.id).unwrap();
        assert!(
            until(Duration::from_secs(5), || !reg.list()[0].alive),
            "killed session stayed alive"
        );
    }

    #[test]
    fn scrollback_is_capped() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture("cat", "/bin/cat", &[])]);
        let session = reg.start("cat", "/tmp", |_| {}, |_| {}).unwrap();
        // A PTY in canonical mode drops input long before 256 KB of echo comes
        // back, so the cap is exercised on the buffer that actually holds it.
        let line = "x".repeat(4096) + "\n";
        let mut buf = Scrollback {
            bytes: Vec::new(),
            alive: true,
        };
        for _ in 0..80 {
            buf.push(line.as_bytes());
        }
        assert!(80 * line.len() > SCROLLBACK_LIMIT);

        let text = buf.text();
        assert!(
            text.len() <= SCROLLBACK_LIMIT,
            "scrollback grew past the cap: {}",
            text.len()
        );
        assert!(!text.is_empty());
        assert!(text.bytes().all(|b| b == b'x' || b == b'\n'));

        // The live process still reports through the same registry.
        reg.write(&session.id, "ping\n").unwrap();
        assert!(
            until(Duration::from_secs(5), || reg
                .scrollback(&session.id)
                .contains("ping")),
            "live scrollback stopped working"
        );
    }

    #[test]
    fn kill_terminates_a_long_running_process() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture("cat", "/bin/cat", &[])]);
        let session = reg.start("cat", "/tmp", |_| {}, |_| {}).unwrap();
        assert!(reg.list()[0].alive);
        reg.kill(&session.id).unwrap();
        assert!(
            until(Duration::from_secs(5), || !reg.list()[0].alive),
            "session stayed alive after kill"
        );
    }

    #[test]
    fn scrollback_drops_from_a_character_boundary() {
        // Multi-byte characters, so a byte-wise trim would cut one in half.
        let glyph = "\u{1F600}";
        let mut buf = Scrollback {
            bytes: Vec::new(),
            alive: true,
        };
        let filler = glyph.repeat(1024) + "\n";
        for _ in 0..80 {
            buf.push(filler.as_bytes());
        }
        assert!(80 * filler.len() > SCROLLBACK_LIMIT);

        let text = buf.text();
        assert!(text.len() <= SCROLLBACK_LIMIT);
        assert!(!text.contains('\u{FFFD}'));
        assert!(text.starts_with(glyph) || text.starts_with('\n'));
    }
}
