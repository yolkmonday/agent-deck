use serde::Serialize;
use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Per-session output kept in memory so a re-mounted terminal view can replay it.
const SCROLLBACK_LIMIT: usize = 256 * 1024;
#[cfg(test)]
const EXIT_POLL_MS: u64 = 20;

/// Sane floor for a PTY size the frontend has not measured yet (e.g. before the
/// first `fit()`). Small enough to never reject a real request, large enough
/// that a shell prompt never wraps mid-word.
const MIN_COLS: u16 = 20;
const MIN_ROWS: u16 = 4;

/// Raw read size per `reader.read()` call.
const READ_CHUNK: usize = 8192;
/// A batch is flushed to scrollback + the attached channel once it hits this
/// size, even if more data is still arriving.
const COALESCE_MAX_BYTES: usize = 64 * 1024;
/// ...or once this much time has passed since the batch's first byte, whichever
/// comes first. Keeps a lone keystroke's echo from waiting behind a stalled
/// firehose, while still merging the many small reads a fast TUI repaint causes.
const COALESCE_MAX_DELAY: Duration = Duration::from_millis(4);
/// How long the one-time login-shell PATH probe gets before we give up. A
/// heavy `.zshrc`/`.bashrc` can be slow; the terminal session itself has no
/// such timeout, only this preflight check does.
#[cfg(unix)]
const LOGIN_PATH_TIMEOUT: Duration = Duration::from_secs(3);

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
pub struct TermExitEvent {
    pub id: String,
    pub code: Option<i32>,
}

/// Carries the exit code from the waiter thread to any caller that polls the
/// registry. Liveness itself is tracked in the shared buffer, not here.
enum Signal {
    Exit(Option<i32>),
}

/// A session's live output receiver, e.g. one that forwards to a Tauri IPC
/// channel. Called with each freshly pushed chunk after `attach`.
type OutputSink = Box<dyn Fn(&[u8]) + Send>;

/// Output buffer shared between the coalescer thread and the registry.
///
/// The coalescer thread owns appending so a slow caller can never stall it: if
/// the buffer only grew when the app asked for it, a full channel would block
/// the reader, the PTY would fill, and the next `write` would deadlock.
///
/// `sink` is the currently attached output channel, if any. It lives behind
/// this same mutex as `bytes` so `TerminalRegistry::attach` can hand a caller
/// the exact scrollback snapshot it is about to start receiving live data on
/// top of, with no gap and no duplicate: nothing else can push into `bytes`
/// while the attach call holds the lock.
#[derive(Default)]
struct Scrollback {
    bytes: Vec<u8>,
    alive: bool,
    sink: Option<OutputSink>,
}

impl Scrollback {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() > SCROLLBACK_LIMIT {
            let mut cut = self.bytes.len() - SCROLLBACK_LIMIT;
            while cut < self.bytes.len() && (self.bytes[cut] & 0xC0) == 0x80 {
                cut += 1;
            }
            self.bytes.drain(..cut);
        }
        if let Some(sink) = &self.sink {
            sink(chunk);
        }
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
    /// The login shell's `PATH` (unix only), resolved once per shell and reused
    /// for every subsequent existence check instead of re-shelling out.
    #[cfg(unix)]
    login_path_cache: std::collections::HashMap<String, String>,
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
///
/// Used only for the profile picker's `available` badge, which reflects this
/// process's own (possibly launchd-minimal) environment. The actual spawn
/// precondition in `start()` checks the login shell's `PATH` instead — see
/// `program_exists_in_path`.
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

/// Same check as `on_path`, but against an arbitrary `PATH`-shaped string
/// instead of this process's inherited environment. Used to validate a
/// program against the login shell's resolved `PATH`.
#[cfg(unix)]
fn program_exists_in_path(program: &str, path_value: &str) -> bool {
    if program.is_empty() {
        return false;
    }
    if program.contains('/') {
        return std::fs::metadata(program)
            .map(|m| m.is_file() && is_executable(&m))
            .unwrap_or(false);
    }
    std::env::split_paths(path_value).any(|dir| {
        std::fs::metadata(dir.join(program))
            .map(|m| m.is_file() && is_executable(&m))
            .unwrap_or(false)
    })
}

fn display_command(profile: &TermProfile) -> String {
    let mut parts = vec![profile.program.clone()];
    parts.extend(profile.args.iter().cloned());
    parts.join(" ")
}

/// The user's login/interactive shell, e.g. from `$SHELL`. Falling back to
/// `/bin/zsh` matches macOS's own default.
#[cfg(unix)]
fn resolve_shell() -> String {
    match std::env::var("SHELL") {
        Ok(shell) if !shell.is_empty() => shell,
        _ => "/bin/zsh".to_string(),
    }
}

/// Runs `shell -l -i -c <script>` to completion or `timeout`, whichever is
/// first, and returns trimmed stdout. Killed and reported as an error on
/// timeout rather than left to run.
#[cfg(unix)]
fn run_shell_script(shell: &str, script: &str, timeout: Duration) -> anyhow::Result<String> {
    let mut child = std::process::Command::new(shell)
        .args(["-l", "-i", "-c", script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let mut out = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                stdout.read_to_string(&mut out)?;
            }
            if !status.success() {
                anyhow::bail!("`{shell} -l -i -c` exited with {status}");
            }
            return Ok(out.trim().to_string());
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("timed out waiting for `{shell}` login shell");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// An interactive shell's rc file is free to print its own banner to stdout
/// (a version notice, a prompt-framework splash, ...) before our script's own
/// output — real dotfiles do this. Markers let us pull just the `PATH` back
/// out regardless of what else landed on stdout first.
#[cfg(unix)]
const PATH_MARKER_START: &str = "__agent_deck_path__";
#[cfg(unix)]
const PATH_MARKER_END: &str = "__agent_deck_path_end__";

/// Resolves the login shell's `PATH` by asking the shell itself, so it sees
/// whatever `.zshrc`/`.bash_profile` etc. actually export — not just this
/// process's own (possibly launchd-minimal) inherited `PATH`.
#[cfg(unix)]
fn resolve_login_path(shell: &str) -> anyhow::Result<String> {
    let script = format!("printf '{PATH_MARKER_START}%s{PATH_MARKER_END}' \"$PATH\"");
    let out = run_shell_script(shell, &script, LOGIN_PATH_TIMEOUT)?;
    let after_start = out
        .find(PATH_MARKER_START)
        .map(|i| &out[i + PATH_MARKER_START.len()..])
        .ok_or_else(|| anyhow::anyhow!("login shell output did not contain the PATH marker"))?;
    let end = after_start
        .find(PATH_MARKER_END)
        .ok_or_else(|| anyhow::anyhow!("login shell output did not contain the PATH end marker"))?;
    Ok(after_start[..end].to_string())
}

/// Wraps `'` as `'\''` and single-quotes the whole word, which is safe for any
/// byte a shell word can contain.
fn shell_quote(word: &str) -> String {
    let mut quoted = String::with_capacity(word.len() + 2);
    quoted.push('\'');
    for ch in word.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

/// Builds the argv for running `program args...` through `shell` as a login,
/// interactive shell. The `-c` script starts with `exec`, which is not one of
/// the tokens shells expand aliases after — so a shell alias for `program`
/// (e.g. `codex` aliased to `codex -s danger-full-access`) is deliberately
/// never applied, the same guarantee the previous direct-spawn approach had.
fn build_login_shell_command(shell: &str, program: &str, args: &[String]) -> (String, Vec<String>) {
    let mut script = String::from("exec ");
    script.push_str(&shell_quote(program));
    for arg in args {
        script.push(' ');
        script.push_str(&shell_quote(arg));
    }
    (
        shell.to_string(),
        vec![
            "-l".to_string(),
            "-i".to_string(),
            "-c".to_string(),
            script,
        ],
    )
}

/// Env additions layered on top of whatever the child inherits. `LANG` is only
/// set when the current one is missing or empty, so a user's own locale is
/// never overridden.
fn build_child_env(existing_lang: Option<&str>) -> Vec<(&'static str, String)> {
    let mut env = vec![
        ("TERM", "xterm-256color".to_string()),
        ("COLORTERM", "truecolor".to_string()),
        ("TERM_PROGRAM", "AgentDeck".to_string()),
    ];
    let lang_missing = existing_lang.map(str::is_empty).unwrap_or(true);
    if lang_missing {
        env.push(("LANG", "en_US.UTF-8".to_string()));
    }
    env
}

/// Runs a coalescer over raw read chunks: blocks for the first chunk of a
/// batch, then keeps folding in whatever else arrives within
/// `COALESCE_MAX_DELAY` (or until `COALESCE_MAX_BYTES`), then flushes once.
/// Returns once `rx` disconnects (the raw reader thread hit EOF or an error),
/// after flushing any partial batch it was still holding.
fn coalesce_loop<F: FnMut(Vec<u8>)>(rx: Receiver<Vec<u8>>, mut flush: F) {
    loop {
        let first = match rx.recv() {
            Ok(chunk) => chunk,
            Err(_) => return,
        };
        let mut batch = first;
        let deadline = Instant::now() + COALESCE_MAX_DELAY;
        let mut disconnected = false;
        while batch.len() < COALESCE_MAX_BYTES {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match rx.recv_timeout(deadline - now) {
                Ok(chunk) => batch.extend_from_slice(&chunk),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        flush(batch);
        if disconnected {
            return;
        }
    }
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
            #[cfg(unix)]
            login_path_cache: std::collections::HashMap::new(),
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
            #[cfg(unix)]
            login_path_cache: std::collections::HashMap::new(),
        }
    }

    pub fn profiles() -> Vec<TermProfile> {
        builtin_profiles()
    }

    #[cfg(unix)]
    fn login_path_for(&mut self, shell: &str) -> anyhow::Result<String> {
        if let Some(cached) = self.login_path_cache.get(shell) {
            return Ok(cached.clone());
        }
        let resolved = resolve_login_path(shell)?;
        self.login_path_cache.insert(shell.to_string(), resolved.clone());
        Ok(resolved)
    }

    /// Builds the command to spawn for `profile`, having already validated the
    /// program is reachable. On unix this runs the program through the user's
    /// login, interactive shell (see `build_login_shell_command`) so it gets a
    /// real shell environment (brew/bun/nvm PATH, `LANG`, etc.) exactly like a
    /// Terminal.app tab would. Elsewhere it is spawned directly, unchanged from
    /// before.
    #[cfg(unix)]
    fn build_command(
        &mut self,
        profile: &TermProfile,
    ) -> anyhow::Result<portable_pty::CommandBuilder> {
        let shell = resolve_shell();
        let login_path = self.login_path_for(&shell)?;
        if !program_exists_in_path(&profile.program, &login_path) {
            anyhow::bail!("program not found on PATH: {}", profile.program);
        }
        let (shell_path, shell_args) =
            build_login_shell_command(&shell, &profile.program, &profile.args);
        let mut cmd = portable_pty::CommandBuilder::new(&shell_path);
        cmd.args(shell_args.iter());
        Ok(cmd)
    }

    #[cfg(not(unix))]
    fn build_command(
        &mut self,
        profile: &TermProfile,
    ) -> anyhow::Result<portable_pty::CommandBuilder> {
        if !on_path(&profile.program) {
            anyhow::bail!("program not found on PATH: {}", profile.program);
        }
        let mut cmd = portable_pty::CommandBuilder::new(&profile.program);
        cmd.args(profile.args.iter());
        Ok(cmd)
    }

    /// Spawns `profile`'s program and wires up its PTY. `on_exit` is called
    /// once, from a dedicated waiter thread, when the child exits. Output does
    /// not flow through a closure here: attach a sink afterwards with `attach`
    /// (a session can run for a while, e.g. after a recover/restart, before
    /// anything attaches to it — output is simply kept in scrollback until then).
    pub fn start<G>(
        &mut self,
        profile_id: &str,
        cwd: &str,
        cols: u16,
        rows: u16,
        on_exit: G,
    ) -> anyhow::Result<TermSession>
    where
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

        let mut cmd = self.build_command(&profile)?;
        cmd.cwd(cwd);
        // When Agent Deck is itself launched from a Claude session, this marker is inherited and
        // the agent we spawn turns transcript saving OFF. The dashboard would then show that
        // session with no activity and no tokens, which is the opposite of the point. Sessions
        // started from here are the user's own, not a nested agent run.
        cmd.env_remove("CLAUDE_CODE_CHILD_SESSION");
        let existing_lang = std::env::var("LANG").ok();
        for (key, value) in build_child_env(existing_lang.as_deref()) {
            cmd.env(key, value);
        }

        let cols = cols.max(MIN_COLS);
        let rows = rows.max(MIN_ROWS);
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system.openpty(portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

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
            sink: None,
        }));

        // Raw reader: does nothing but block on `read` and forward chunks. Kept
        // tiny and decode-free so it never falls behind the PTY.
        let (raw_tx, raw_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; READ_CHUNK];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if raw_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Coalescer: owns appending to scrollback and forwarding to whatever
        // sink is attached, so a slow caller can never stall the raw reader
        // above (and, in turn, the PTY itself).
        std::thread::spawn({
            let buffer = buffer.clone();
            move || {
                coalesce_loop(raw_rx, |batch| {
                    buffer.lock().unwrap().push(&batch);
                });
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

    /// Attaches `sink` as the session's live output receiver. Under the
    /// session's buffer lock, `sink` is first called once with the full
    /// current scrollback (the snapshot a re-mounted view needs to replay),
    /// then stored as the ongoing sink for every future `push`. Because both
    /// happen while the lock is held, nothing pushed by the reader can land
    /// between the snapshot and the first live call: no gap, no duplicate. A
    /// second `attach` for the same id simply replaces the previous sink.
    pub fn attach(&mut self, id: &str, sink: OutputSink) -> anyhow::Result<()> {
        let session = Self::slot(&mut self.sessions, id)?;
        let mut buf = session.buffer.lock().unwrap();
        sink(&buf.bytes);
        buf.sink = Some(sink);
        Ok(())
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

    impl TerminalRegistry {
        /// Test-only peek at raw scrollback bytes without going through `attach`.
        fn snapshot_bytes(&self, id: &str) -> Vec<u8> {
            self.sessions
                .iter()
                .find(|s| s.info.id == id)
                .map(|s| s.buffer.lock().unwrap().bytes.clone())
                .unwrap_or_default()
        }
    }

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
            .start("nope", "/tmp", 80, 24, |_| {})
            .unwrap_err()
            .to_string();
        assert!(err.contains("nope"), "unexpected error: {err}");
    }

    #[test]
    fn start_rejects_missing_cwd() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture("echo", "/bin/echo", &[])]);
        let err = reg
            .start("echo", "/no/such/dir/xyz", 80, 24, |_| {})
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
            .start("ghost", "/tmp", 80, 24, |_| {})
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
        let exited = Arc::new(Mutex::new(false));
        let e2 = exited.clone();
        let session = reg
            .start("echo", "/tmp", 80, 24, move |_| *e2.lock().unwrap() = true)
            .unwrap();
        assert_eq!(session.command, "/bin/echo hello");

        let data = Arc::new(Mutex::new(Vec::<u8>::new()));
        let d2 = data.clone();
        reg.attach(
            &session.id,
            Box::new(move |chunk: &[u8]| d2.lock().unwrap().extend_from_slice(chunk)),
        )
        .unwrap();

        assert!(
            until(Duration::from_secs(5), || String::from_utf8_lossy(
                &data.lock().unwrap()
            )
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
        let session = reg.start("cat", "/tmp", 80, 24, |_| {}).unwrap();
        let data = Arc::new(Mutex::new(Vec::<u8>::new()));
        let d2 = data.clone();
        reg.attach(
            &session.id,
            Box::new(move |chunk: &[u8]| d2.lock().unwrap().extend_from_slice(chunk)),
        )
        .unwrap();
        reg.write(&session.id, "ping\n").unwrap();
        assert!(
            until(Duration::from_secs(5), || String::from_utf8_lossy(
                &data.lock().unwrap()
            )
            .contains("ping")),
            "ping never came back"
        );
    }

    #[test]
    fn attach_sends_snapshot_then_streams_new_data_without_duplication() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture("cat", "/bin/cat", &[])]);
        let session = reg.start("cat", "/tmp", 80, 24, |_| {}).unwrap();
        reg.write(&session.id, "before\n").unwrap();
        assert!(
            until(Duration::from_secs(5), || reg
                .snapshot_bytes(&session.id)
                .len()
                >= 7),
            "process never echoed the priming write"
        );
        let expected_snapshot = reg.snapshot_bytes(&session.id);

        let received = Arc::new(Mutex::new(Vec::<u8>::new()));
        let r2 = received.clone();
        reg.attach(
            &session.id,
            Box::new(move |chunk: &[u8]| r2.lock().unwrap().extend_from_slice(chunk)),
        )
        .unwrap();
        // The very first bytes a freshly attached sink sees must be exactly the
        // scrollback snapshot taken under the same lock: no gap, no duplicate.
        assert_eq!(
            received.lock().unwrap().as_slice(),
            expected_snapshot.as_slice()
        );

        reg.write(&session.id, "after\n").unwrap();
        assert!(
            until(Duration::from_secs(5), || String::from_utf8_lossy(
                &received.lock().unwrap()
            )
            .contains("after")),
            "no live data streamed after attach"
        );
        let full = received.lock().unwrap().clone();
        assert_eq!(
            String::from_utf8_lossy(&full).matches("before").count(),
            1,
            "snapshot was replayed a second time once live data started: {full:?}"
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
        let live = reg.start("cat", "/tmp", 80, 24, |_| {}).unwrap();
        assert!(reg.list().iter().any(|s| s.id == live.id && s.alive));

        let gone = reg.start("echo", "/tmp", 80, 24, |_| {}).unwrap();
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
        let session = reg.start("cat", "/tmp", 80, 24, |_| {}).unwrap();
        // A PTY in canonical mode drops input long before 256 KB of echo comes
        // back, so the cap is exercised on the buffer that actually holds it.
        let line = "x".repeat(4096) + "\n";
        let mut buf = Scrollback::default();
        for _ in 0..80 {
            buf.push(line.as_bytes());
        }
        assert!(80 * line.len() > SCROLLBACK_LIMIT);
        assert!(
            buf.bytes.len() <= SCROLLBACK_LIMIT,
            "scrollback grew past the cap: {}",
            buf.bytes.len()
        );
        assert!(!buf.bytes.is_empty());
        assert!(buf.bytes.iter().all(|&b| b == b'x' || b == b'\n'));

        // The live process still reports through the same registry.
        let data = Arc::new(Mutex::new(Vec::<u8>::new()));
        let d2 = data.clone();
        reg.attach(
            &session.id,
            Box::new(move |chunk: &[u8]| d2.lock().unwrap().extend_from_slice(chunk)),
        )
        .unwrap();
        reg.write(&session.id, "ping\n").unwrap();
        assert!(
            until(Duration::from_secs(5), || String::from_utf8_lossy(
                &data.lock().unwrap()
            )
            .contains("ping")),
            "live scrollback stopped working"
        );
    }

    #[test]
    fn kill_terminates_a_long_running_process() {
        let mut reg = TerminalRegistry::with_profiles(vec![fixture("cat", "/bin/cat", &[])]);
        let session = reg.start("cat", "/tmp", 80, 24, |_| {}).unwrap();
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
        let mut buf = Scrollback::default();
        let filler = glyph.repeat(1024) + "\n";
        for _ in 0..80 {
            buf.push(filler.as_bytes());
        }
        assert!(80 * filler.len() > SCROLLBACK_LIMIT);
        assert!(buf.bytes.len() <= SCROLLBACK_LIMIT);
        assert!(
            buf.bytes
                .first()
                .map(|&b| (b & 0xC0) != 0x80)
                .unwrap_or(true),
            "scrollback starts mid multi-byte character"
        );
    }

    #[test]
    fn coalesce_loop_merges_a_fast_burst_into_one_batch() {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let batches: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let b2 = batches.clone();
        let handle = std::thread::spawn(move || {
            coalesce_loop(rx, |batch| b2.lock().unwrap().push(batch));
        });
        tx.send(b"a".to_vec()).unwrap();
        tx.send(b"b".to_vec()).unwrap();
        tx.send(b"c".to_vec()).unwrap();
        // Let the coalesce window close, then send a second, separate burst.
        std::thread::sleep(Duration::from_millis(30));
        tx.send(b"d".to_vec()).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        drop(tx);
        handle.join().unwrap();

        let batches = batches.lock().unwrap();
        assert_eq!(batches.len(), 2, "expected two coalesced batches, got {batches:?}");
        assert_eq!(batches[0], b"abc");
        assert_eq!(batches[1], b"d");
    }

    #[test]
    fn coalesce_loop_flushes_at_the_byte_cap_even_mid_burst() {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let batches: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
        let b2 = batches.clone();
        let handle = std::thread::spawn(move || {
            coalesce_loop(rx, |batch| b2.lock().unwrap().push(batch.len()));
        });
        tx.send(vec![0u8; COALESCE_MAX_BYTES]).unwrap();
        tx.send(vec![1u8; 10]).unwrap();
        drop(tx);
        handle.join().unwrap();

        let batches = batches.lock().unwrap();
        assert_eq!(*batches, vec![COALESCE_MAX_BYTES, 10]);
    }

    #[cfg(unix)]
    #[test]
    fn shell_quote_wraps_and_escapes_single_quotes() {
        assert_eq!(shell_quote("simple"), "'simple'");
        assert_eq!(shell_quote("with space"), "'with space'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
    }

    #[cfg(unix)]
    #[test]
    fn build_login_shell_command_execs_with_quoted_args_and_never_expands_aliases() {
        let (shell, argv) = build_login_shell_command(
            "/bin/zsh",
            "codex",
            &["-s".to_string(), "danger full access".to_string()],
        );
        assert_eq!(shell, "/bin/zsh");
        assert_eq!(argv, vec![
            "-l".to_string(),
            "-i".to_string(),
            "-c".to_string(),
            "exec 'codex' '-s' 'danger full access'".to_string(),
        ]);
    }

    #[cfg(unix)]
    #[test]
    fn build_child_env_sets_lang_only_when_missing() {
        for env in [build_child_env(None), build_child_env(Some(""))] {
            assert!(
                env.iter()
                    .any(|(k, v)| *k == "LANG" && v == "en_US.UTF-8"),
                "expected LANG to be filled in: {env:?}"
            );
        }
        let existing = build_child_env(Some("id_ID.UTF-8"));
        assert!(
            !existing.iter().any(|(k, _)| *k == "LANG"),
            "existing LANG must not be overridden: {existing:?}"
        );

        for env in [
            build_child_env(None),
            build_child_env(Some("")),
            build_child_env(Some("id_ID.UTF-8")),
        ] {
            assert!(env.contains(&("TERM", "xterm-256color".to_string())));
            assert!(env.contains(&("COLORTERM", "truecolor".to_string())));
            assert!(env.contains(&("TERM_PROGRAM", "AgentDeck".to_string())));
        }
    }

    #[cfg(unix)]
    #[test]
    fn program_exists_in_path_checks_the_given_path_value_not_the_process_env() {
        assert!(program_exists_in_path("sh", "/bin:/usr/bin"));
        assert!(!program_exists_in_path(
            "definitely-not-a-real-binary-xyz",
            "/bin:/usr/bin"
        ));
        assert!(!program_exists_in_path("", "/bin:/usr/bin"));
        assert!(program_exists_in_path("/bin/sh", "/nonexistent-dir-xyz"));
    }

    #[cfg(unix)]
    #[test]
    fn run_shell_script_returns_trimmed_stdout() {
        // An interactive shell's rc file may print its own banner before our
        // script's own output runs, so this only asserts our part landed —
        // not that stdout is nothing else. `resolve_login_path`'s marker
        // extraction is what makes that noise harmless for real callers.
        let shell = resolve_shell();
        let out = run_shell_script(&shell, "echo -n hello", Duration::from_secs(3)).unwrap();
        assert!(out.ends_with("hello"), "unexpected output: {out:?}");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_login_path_ignores_rc_banner_noise() {
        // This machine's own dotfiles print a banner ("lean-ctx: ON") on
        // interactive shell startup, which is exactly the case the marker
        // extraction in `resolve_login_path` exists to survive.
        let path = resolve_login_path(&resolve_shell()).unwrap();
        assert!(!path.is_empty());
        assert!(!path.contains(PATH_MARKER_START));
        assert!(!path.contains(PATH_MARKER_END));
        assert!(path.contains('/'), "does not look like a PATH: {path:?}");
    }

    #[cfg(unix)]
    #[test]
    fn run_shell_script_times_out_on_a_hanging_command() {
        let shell = resolve_shell();
        let start = Instant::now();
        let err = run_shell_script(&shell, "sleep 5", Duration::from_millis(150));
        assert!(err.is_err());
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "did not honor the timeout: took {:?}",
            start.elapsed()
        );
    }
}
