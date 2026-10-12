//! JSON-lines-over-stdio client for the QVAC sidecar (`sidecar/worker.js`).
//!
//! Protocol (docs/qvac-integration.md §4.1): one JSON object per line.
//! Rust → sidecar `{"id":N,"method":"…","params":{…}}`; sidecar → Rust
//! `{"id":N,"result":…}` / `{"id":N,"error":{"code","message"}}` /
//! `{"event":"progress","id":N,"data":…}`; the first line after spawn is
//! `{"ready":true,"version":"…"}`.
//!
//! The client is synchronous (std threads + channels) so it can be used from
//! the blocking PII pipeline. It spawns the sidecar lazily, tags every request
//! with an id, enforces per-request timeouts, supports cancellation, detects
//! crashes and restarts with exponential backoff (at most `max_restarts`
//! restarts per `restart_window`, after which it reports itself unavailable for
//! the rest of the session), and shuts the process down cleanly.
//!
//! Nothing in here logs request or response payloads: only methods, ids,
//! timings and sidecar stderr (which the worker keeps metadata-only).

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Protocol version the Rust side speaks (checked against `hello`).
pub const PROTOCOL_VERSION: u64 = 1;

/// How to start the sidecar process.
#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    /// Start from an empty environment (only `env` is passed).
    pub clear_env: bool,
}

/// Client tuning. Defaults match the design doc; tests shrink them.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Time allowed for the `ready` line after spawn (first run on Windows
    /// can be slow, upstream issue #4394).
    pub startup_timeout: Duration,
    pub hello_timeout: Duration,
    /// Delay before restart n (0-based) is `backoff_base * 2^n`.
    pub backoff_base: Duration,
    pub max_restarts: usize,
    pub restart_window: Duration,
    /// Grace period for `shutdown` before the process is killed.
    pub shutdown_grace: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(60),
            hello_timeout: Duration::from_secs(15),
            backoff_base: Duration::from_millis(500),
            max_restarts: 3,
            restart_window: Duration::from_secs(5 * 60),
            shutdown_grace: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum QvacError {
    /// Sidecar missing, shut down, or crashed too often this session.
    Unavailable(String),
    /// Could not start the process or it failed the handshake.
    Spawn(String),
    Timeout,
    Cancelled,
    /// The process exited while the request was in flight.
    Crashed,
    /// The sidecar answered with an error object.
    Remote { code: String, message: String },
    Protocol(String),
}

impl std::fmt::Display for QvacError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QvacError::Unavailable(r) => write!(f, "AI engine unavailable: {}", r),
            QvacError::Spawn(r) => write!(f, "AI engine failed to start: {}", r),
            QvacError::Timeout => write!(f, "AI engine request timed out"),
            QvacError::Cancelled => write!(f, "AI engine request cancelled"),
            QvacError::Crashed => write!(f, "AI engine process exited unexpectedly"),
            QvacError::Remote { code, message } => write!(f, "AI engine error {}: {}", code, message),
            QvacError::Protocol(r) => write!(f, "AI engine protocol error: {}", r),
        }
    }
}

impl std::error::Error for QvacError {}

/// Cooperative cancellation flag shared between a caller and a request.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Callback for `{"event": …}` lines addressed to a request.
pub type EventFn = Arc<dyn Fn(&Value) + Send + Sync>;

#[derive(Default, Clone)]
pub struct CallOptions {
    pub timeout: Option<Duration>,
    pub cancel: Option<CancelToken>,
    pub on_event: Option<EventFn>,
}

impl CallOptions {
    pub fn timeout(d: Duration) -> Self {
        Self { timeout: Some(d), ..Default::default() }
    }
}

type Reply = Result<Value, QvacError>;

struct Pending {
    tx: Sender<Reply>,
    on_event: Option<EventFn>,
}

type PendingMap = Arc<Mutex<HashMap<u64, Pending>>>;

struct Process {
    child: Child,
    stdin: ChildStdin,
    pending: PendingMap,
    alive: Arc<AtomicBool>,
    version: String,
}

impl Process {
    fn write_line(&mut self, v: &Value) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(v).map_err(std::io::Error::other)?;
        line.push(b'\n');
        self.stdin.write_all(&line)?;
        self.stdin.flush()
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.alive.store(false, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct Lifecycle {
    /// A process has been started at least once (so the next start after a
    /// crash is a *restart*).
    started_once: bool,
    restarts: Vec<Instant>,
    unavailable: Option<String>,
    /// Bumped on every successful spawn; lets callers know that models they
    /// loaded into an earlier process are gone.
    generation: u64,
}

pub struct QvacClient {
    spec: SpawnSpec,
    cfg: ClientConfig,
    proc: Mutex<Option<Process>>,
    life: Mutex<Lifecycle>,
    next_id: AtomicU64,
}

impl QvacClient {
    pub fn new(spec: SpawnSpec, cfg: ClientConfig) -> Self {
        Self {
            spec,
            cfg,
            proc: Mutex::new(None),
            life: Mutex::new(Lifecycle::default()),
            next_id: AtomicU64::new(1),
        }
    }

    /// False once the restart cap was hit or the client was shut down.
    pub fn is_available(&self) -> bool {
        self.life.lock().unwrap().unavailable.is_none()
    }

    pub fn unavailable_reason(&self) -> Option<String> {
        self.life.lock().unwrap().unavailable.clone()
    }

    pub fn is_running(&self) -> bool {
        self.proc
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| p.alive.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    /// Spawn generation of the current process (0 = never started).
    pub fn generation(&self) -> u64 {
        self.life.lock().unwrap().generation
    }

    pub fn version(&self) -> Option<String> {
        self.proc.lock().unwrap().as_ref().map(|p| p.version.clone())
    }

    /// Start the sidecar now if it is not running (normally lazy).
    pub fn ensure_started(&self) -> Result<(), QvacError> {
        self.ensure_running().map(|_| ())
    }

    /// Send a request and wait for its reply.
    pub fn call(&self, method: &str, params: Value, opts: CallOptions) -> Result<Value, QvacError> {
        let timeout = opts.timeout.unwrap_or(Duration::from_secs(120));
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        {
            let mut guard = self.ensure_running()?;
            let p = guard.as_mut().expect("ensure_running returned a process");
            p.pending.lock().unwrap().insert(id, Pending { tx, on_event: opts.on_event.clone() });
            if let Err(e) = p.write_line(&json!({"id": id, "method": method, "params": params})) {
                p.pending.lock().unwrap().remove(&id);
                log::warn!("qvac: write to sidecar failed ({}); treating as crash", e.kind());
                p.alive.store(false, Ordering::SeqCst);
                return Err(QvacError::Crashed);
            }
        }
        log::debug!("qvac: -> #{} {}", id, method);
        self.wait_reply(id, method, rx, timeout, opts.cancel.as_ref())
    }

    fn wait_reply(
        &self,
        id: u64,
        method: &str,
        rx: Receiver<Reply>,
        timeout: Duration,
        cancel: Option<&CancelToken>,
    ) -> Result<Value, QvacError> {
        let started = Instant::now();
        let tick = Duration::from_millis(25);
        loop {
            match rx.recv_timeout(tick) {
                Ok(reply) => {
                    log::debug!("qvac: <- #{} {} in {} ms", id, method, started.elapsed().as_millis());
                    return reply;
                }
                Err(RecvTimeoutError::Disconnected) => return Err(QvacError::Crashed),
                Err(RecvTimeoutError::Timeout) => {
                    if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
                        self.abandon(id);
                        return Err(QvacError::Cancelled);
                    }
                    if started.elapsed() >= timeout {
                        log::warn!("qvac: #{} {} timed out after {:?}", id, method, timeout);
                        self.abandon(id);
                        return Err(QvacError::Timeout);
                    }
                }
            }
        }
    }

    /// Forget a request locally and ask the sidecar to cancel it.
    fn abandon(&self, id: u64) {
        let mut guard = self.proc.lock().unwrap();
        if let Some(p) = guard.as_mut() {
            p.pending.lock().unwrap().remove(&id);
            if p.alive.load(Ordering::SeqCst) {
                let cid = self.next_id.fetch_add(1, Ordering::SeqCst);
                // Fire-and-forget; the reply (if any) has no pending entry.
                let _ = p.write_line(&json!({"id": cid, "method": "cancel", "params": {"target": id}}));
            }
        }
    }

    /// Stop the process without marking the client unavailable (used for
    /// idle unload). The next call spawns a fresh process and is not counted
    /// as a crash restart.
    pub fn stop(&self) {
        let mut guard = self.proc.lock().unwrap();
        if let Some(mut p) = guard.take() {
            self.graceful_exit(&mut p);
        }
        // A deliberate stop is not a crash.
        self.life.lock().unwrap().started_once = false;
    }

    /// Shut down for good (app exit). Further calls return `Unavailable`.
    pub fn shutdown(&self) {
        self.life.lock().unwrap().unavailable = Some("shut down".into());
        let mut guard = self.proc.lock().unwrap();
        if let Some(mut p) = guard.take() {
            self.graceful_exit(&mut p);
        }
    }

    fn graceful_exit(&self, p: &mut Process) {
        if p.alive.load(Ordering::SeqCst) {
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            let _ = p.write_line(&json!({"id": id, "method": "shutdown", "params": {}}));
            let deadline = Instant::now() + self.cfg.shutdown_grace;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = p.child.try_wait() {
                    p.alive.store(false, Ordering::SeqCst);
                    log::info!("qvac: sidecar exited cleanly");
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
            log::warn!("qvac: sidecar did not exit within {:?}; killing", self.cfg.shutdown_grace);
        }
        p.kill();
    }

    /// Return a guard holding a live process, spawning or restarting as needed.
    fn ensure_running(&self) -> Result<MutexGuard<'_, Option<Process>>, QvacError> {
        let mut guard = self.proc.lock().unwrap();
        if let Some(reason) = self.life.lock().unwrap().unavailable.clone() {
            return Err(QvacError::Unavailable(reason));
        }
        if let Some(p) = guard.as_mut() {
            if p.alive.load(Ordering::SeqCst) {
                return Ok(guard);
            }
            // Dead process: reap it; the next start counts as a restart.
            let status = p.child.wait().ok();
            log::warn!("qvac: sidecar exited unexpectedly (status {:?})", status.and_then(|s| s.code()));
            *guard = None;
        }

        // Restart accounting.
        {
            let mut life = self.life.lock().unwrap();
            if life.started_once {
                let now = Instant::now();
                let window = self.cfg.restart_window;
                life.restarts.retain(|t| now.duration_since(*t) < window);
                if life.restarts.len() >= self.cfg.max_restarts {
                    let reason = format!(
                        "sidecar crashed {} times within {} s; disabled for this session",
                        life.restarts.len() + 1,
                        window.as_secs()
                    );
                    log::error!("qvac: {}", reason);
                    life.unavailable = Some(reason.clone());
                    return Err(QvacError::Unavailable(reason));
                }
                let delay = self.cfg.backoff_base * 2u32.pow(life.restarts.len() as u32);
                life.restarts.push(now);
                drop(life);
                log::info!("qvac: restarting sidecar after {:?}", delay);
                thread::sleep(delay);
            }
        }

        let process = self.spawn()?;
        {
            let mut life = self.life.lock().unwrap();
            life.started_once = true;
            life.generation += 1;
        }
        *guard = Some(process);
        Ok(guard)
    }

    fn spawn(&self) -> Result<Process, QvacError> {
        let started = Instant::now();
        let mut cmd = Command::new(&self.spec.program);
        cmd.args(&self.spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.spec.clear_env {
            cmd.env_clear();
        }
        for (k, v) in &self.spec.env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| {
            // Count a failed start as a start so repeated failures hit the cap.
            self.life.lock().unwrap().started_once = true;
            QvacError::Spawn(format!("{}", e.kind()))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| QvacError::Spawn("no stdin".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| QvacError::Spawn("no stdout".into()))?;
        let stderr = child.stderr.take().ok_or_else(|| QvacError::Spawn("no stderr".into()))?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready_rx) = mpsc::channel::<Value>();

        // stdout reader: dispatch replies/events, ignore anything non-JSON.
        {
            let pending = Arc::clone(&pending);
            let alive = Arc::clone(&alive);
            thread::Builder::new()
                .name("qvac-stdout".into())
                .spawn(move || read_stdout(stdout, pending, alive, ready_tx))
                .map_err(|e| QvacError::Spawn(format!("reader thread: {}", e)))?;
        }
        // stderr → app log (the worker keeps stderr metadata-only).
        thread::Builder::new()
            .name("qvac-stderr".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    match line {
                        Ok(l) if !l.trim().is_empty() => {
                            let l: String = l.chars().take(300).collect();
                            log::info!("[qvac-sidecar] {}", l);
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| QvacError::Spawn(format!("stderr thread: {}", e)))?;

        let mut process = Process { child, stdin, pending, alive, version: String::new() };

        // Wait for {"ready":true}.
        let ready = match ready_rx.recv_timeout(self.cfg.startup_timeout) {
            Ok(v) => v,
            Err(e) => {
                process.kill();
                self.life.lock().unwrap().started_once = true;
                return Err(QvacError::Spawn(match e {
                    RecvTimeoutError::Timeout => "no ready line before startup timeout".into(),
                    RecvTimeoutError::Disconnected => "sidecar exited before ready".into(),
                }));
            }
        };
        process.version = ready.get("version").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

        // Handshake.
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        process.pending.lock().unwrap().insert(id, Pending { tx, on_event: None });
        if process.write_line(&json!({"id": id, "method": "hello", "params": {}})).is_err() {
            process.kill();
            self.life.lock().unwrap().started_once = true;
            return Err(QvacError::Spawn("handshake write failed".into()));
        }
        let hello = match rx.recv_timeout(self.cfg.hello_timeout) {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                process.kill();
                self.life.lock().unwrap().started_once = true;
                return Err(QvacError::Spawn(format!("handshake failed: {}", e)));
            }
            Err(_) => {
                process.kill();
                self.life.lock().unwrap().started_once = true;
                return Err(QvacError::Spawn("handshake timed out".into()));
            }
        };
        let proto = hello.get("protocol").and_then(|v| v.as_u64()).unwrap_or(0);
        if proto != PROTOCOL_VERSION {
            process.kill();
            // A version mismatch will not fix itself by restarting.
            let reason = format!("protocol mismatch (sidecar {}, app {})", proto, PROTOCOL_VERSION);
            self.life.lock().unwrap().unavailable = Some(reason.clone());
            return Err(QvacError::Unavailable(reason));
        }
        log::info!(
            "qvac: sidecar ready in {} ms ({})",
            started.elapsed().as_millis(),
            process.version
        );
        Ok(process)
    }
}

impl Drop for QvacClient {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.proc.lock() {
            if let Some(mut p) = guard.take() {
                self.graceful_exit(&mut p);
            }
        }
    }
}

fn read_stdout(
    stdout: std::process::ChildStdout,
    pending: PendingMap,
    alive: Arc<AtomicBool>,
    ready_tx: Sender<Value>,
) {
    let mut ready_tx = Some(ready_tx);
    let mut ignored = 0usize;
    for line in BufReader::new(stdout).lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let msg: Value = match serde_json::from_str(line.trim()) {
            Ok(v @ Value::Object(_)) => v,
            _ => {
                if !line.trim().is_empty() {
                    ignored += 1;
                    log::debug!("qvac: ignored non-protocol stdout line ({} bytes)", line.len());
                }
                continue;
            }
        };
        if msg.get("ready").and_then(|v| v.as_bool()) == Some(true) {
            if let Some(tx) = ready_tx.take() {
                let _ = tx.send(msg);
            }
            continue;
        }
        let id = match msg.get("id").and_then(|v| v.as_u64()) {
            Some(id) => id,
            None => continue,
        };
        if msg.get("event").is_some() {
            let cb = pending.lock().unwrap().get(&id).and_then(|p| p.on_event.clone());
            if let Some(cb) = cb {
                cb(msg.get("data").unwrap_or(&Value::Null));
            }
            continue;
        }
        let entry = pending.lock().unwrap().remove(&id);
        if let Some(entry) = entry {
            let reply = if let Some(err) = msg.get("error") {
                Err(remote_error(err))
            } else {
                Ok(msg.get("result").cloned().unwrap_or(Value::Null))
            };
            let _ = entry.tx.send(reply);
        }
    }
    // EOF: the process is gone. Fail everything in flight.
    alive.store(false, Ordering::SeqCst);
    if ignored > 0 {
        log::debug!("qvac: {} non-protocol stdout lines were ignored", ignored);
    }
    let drained: Vec<Pending> = pending.lock().unwrap().drain().map(|(_, p)| p).collect();
    for p in drained {
        let _ = p.tx.send(Err(QvacError::Crashed));
    }
}

fn remote_error(err: &Value) -> QvacError {
    let code = err.get("code").and_then(|v| v.as_str()).unwrap_or("INTERNAL").to_string();
    let message = err.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
    match code.as_str() {
        "CANCELLED" => QvacError::Cancelled,
        _ => QvacError::Remote { code, message },
    }
}

#[cfg(test)]
mod tests {
    //! The fake sidecar is this test binary itself: `fake_sidecar_entry` is a
    //! test that, when `QVAC_FAKE_SIDECAR=1` is set, speaks the protocol on
    //! stdio instead of returning. libtest's own "running 1 test" banner on
    //! stdout doubles as the non-JSON noise the client must ignore.
    use super::*;
    use std::io::BufRead;

    const FAKE_ENV: &str = "QVAC_FAKE_SIDECAR";

    fn fake_spec(extra_env: &[(&str, &str)]) -> SpawnSpec {
        let exe = std::env::current_exe().unwrap();
        let mut env = vec![(OsString::from(FAKE_ENV), OsString::from("1"))];
        for (k, v) in extra_env {
            env.push((OsString::from(k), OsString::from(v)));
        }
        SpawnSpec {
            program: exe,
            args: vec![
                "--exact".into(),
                "qvac::client::tests::fake_sidecar_entry".into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            env,
            clear_env: false,
        }
    }

    fn fast_cfg() -> ClientConfig {
        ClientConfig {
            startup_timeout: Duration::from_secs(20),
            hello_timeout: Duration::from_secs(5),
            backoff_base: Duration::from_millis(5),
            max_restarts: 3,
            restart_window: Duration::from_secs(300),
            shutdown_grace: Duration::from_secs(2),
        }
    }

    #[test]
    fn fake_sidecar_entry() {
        if std::env::var(FAKE_ENV).is_err() {
            return; // normal test run: nothing to do
        }
        run_fake_sidecar();
    }

    fn run_fake_sidecar() -> ! {
        use std::collections::HashSet;
        let out = Arc::new(Mutex::new(std::io::stdout()));
        let cancelled: Arc<Mutex<HashSet<u64>>> = Arc::new(Mutex::new(HashSet::new()));
        let send = {
            let out = Arc::clone(&out);
            move |v: Value| {
                let mut o = out.lock().unwrap();
                let _ = writeln!(o, "{}", v);
                let _ = o.flush();
            }
        };
        if std::env::var("QVAC_FAKE_DIE_ON_START").is_ok() {
            std::process::exit(7);
        }
        // libtest has printed "test <name> ... " without a newline; end that
        // line so the ready message starts on its own line.
        {
            let mut o = out.lock().unwrap();
            let _ = writeln!(o);
        }
        send(json!({"ready": true, "version": "fake 0.0.0"}));
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = match line { Ok(l) => l, Err(_) => break };
            let req: Value = match serde_json::from_str(&line) { Ok(v) => v, Err(_) => continue };
            let id = req["id"].as_u64().unwrap_or(0);
            let params = req["params"].clone();
            match req["method"].as_str().unwrap_or("") {
                "hello" => send(json!({"id": id, "result": {"protocol": 1}})),
                "echo" => send(json!({"id": id, "result": params})),
                "noise" => {
                    {
                        let mut o = out.lock().unwrap();
                        let _ = writeln!(o, "this is not json");
                        let _ = writeln!(o, "[1,2,3]");
                        let _ = writeln!(o, "{{\"unrelated\":true}}");
                        let _ = o.flush();
                    }
                    send(json!({"id": id, "result": "after-noise"}));
                }
                "events" => {
                    send(json!({"event": "progress", "id": id, "data": {"tokens": 1}}));
                    send(json!({"event": "progress", "id": id, "data": {"tokens": 2}}));
                    send(json!({"id": id, "result": "done"}));
                }
                "sleep" => {
                    let ms = params["ms"].as_u64().unwrap_or(1000);
                    let send = send.clone();
                    let cancelled = Arc::clone(&cancelled);
                    thread::spawn(move || {
                        let end = Instant::now() + Duration::from_millis(ms);
                        while Instant::now() < end {
                            if cancelled.lock().unwrap().contains(&id) {
                                send(json!({"id": id, "error": {"code": "CANCELLED", "message": "cancelled"}}));
                                return;
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        send(json!({"id": id, "result": {"slept": ms}}));
                    });
                }
                "cancel" => {
                    let target = params["target"].as_u64().unwrap_or(0);
                    cancelled.lock().unwrap().insert(target);
                    send(json!({"id": id, "result": {"cancelled": true}}));
                }
                "fail" => send(json!({"id": id, "error": {"code": "BAD_PARAMS", "message": "nope"}})),
                "crash" => std::process::exit(3),
                "shutdown" => {
                    send(json!({"id": id, "result": {"bye": true}}));
                    std::process::exit(0);
                }
                _ => send(json!({"id": id, "error": {"code": "METHOD_NOT_ALLOWED", "message": ""}})),
            }
        }
        std::process::exit(0);
    }

    fn quick() -> CallOptions {
        CallOptions::timeout(Duration::from_secs(10))
    }

    #[test]
    fn normal_call_and_lazy_spawn() {
        let c = QvacClient::new(fake_spec(&[]), fast_cfg());
        assert!(!c.is_running(), "spawn must be lazy");
        let r = c.call("echo", json!({"a": 1}), quick()).unwrap();
        assert_eq!(r, json!({"a": 1}));
        assert!(c.is_running());
        assert_eq!(c.generation(), 1);
        assert_eq!(c.version().as_deref(), Some("fake 0.0.0"));
        let err = c.call("fail", json!({}), quick()).unwrap_err();
        assert_eq!(err, QvacError::Remote { code: "BAD_PARAMS".into(), message: "nope".into() });
        c.shutdown();
        assert!(!c.is_running());
        assert!(matches!(c.call("echo", json!({}), quick()), Err(QvacError::Unavailable(_))));
    }

    #[test]
    fn events_are_delivered() {
        let c = QvacClient::new(fake_spec(&[]), fast_cfg());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = Arc::clone(&seen);
        let opts = CallOptions {
            timeout: Some(Duration::from_secs(10)),
            on_event: Some(Arc::new(move |d: &Value| s2.lock().unwrap().push(d["tokens"].as_u64().unwrap()))),
            ..Default::default()
        };
        assert_eq!(c.call("events", json!({}), opts).unwrap(), json!("done"));
        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn non_json_stdout_lines_are_ignored() {
        let c = QvacClient::new(fake_spec(&[]), fast_cfg());
        assert_eq!(c.call("noise", json!({}), quick()).unwrap(), json!("after-noise"));
        // The process is still healthy afterwards.
        assert_eq!(c.call("echo", json!(5), quick()).unwrap(), json!(5));
        assert_eq!(c.generation(), 1);
    }

    #[test]
    fn timeout_cancels_and_client_stays_usable() {
        let c = QvacClient::new(fake_spec(&[]), fast_cfg());
        let t = Instant::now();
        let err = c
            .call("sleep", json!({"ms": 5000}), CallOptions::timeout(Duration::from_millis(200)))
            .unwrap_err();
        assert_eq!(err, QvacError::Timeout);
        assert!(t.elapsed() < Duration::from_secs(3));
        assert_eq!(c.call("echo", json!("ok"), quick()).unwrap(), json!("ok"));
        assert_eq!(c.generation(), 1, "a timeout must not restart the process");
    }

    #[test]
    fn cancel_token_aborts_request() {
        let c = Arc::new(QvacClient::new(fake_spec(&[]), fast_cfg()));
        c.ensure_started().unwrap();
        let token = CancelToken::new();
        let t2 = token.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            t2.cancel();
        });
        let t = Instant::now();
        let err = c
            .call("sleep", json!({"ms": 5000}), CallOptions { timeout: Some(Duration::from_secs(10)), cancel: Some(token), on_event: None })
            .unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err, QvacError::Cancelled);
        assert!(t.elapsed() < Duration::from_secs(3));
        assert_eq!(c.call("echo", json!(1), quick()).unwrap(), json!(1));
    }

    #[test]
    fn concurrent_requests_are_matched_by_id() {
        let c = Arc::new(QvacClient::new(fake_spec(&[]), fast_cfg()));
        c.ensure_started().unwrap();
        let slow = {
            let c = Arc::clone(&c);
            thread::spawn(move || c.call("sleep", json!({"ms": 300}), quick()))
        };
        thread::sleep(Duration::from_millis(30));
        assert_eq!(c.call("echo", json!("fast"), quick()).unwrap(), json!("fast"));
        assert_eq!(slow.join().unwrap().unwrap(), json!({"slept": 300}));
    }

    #[test]
    fn crash_then_restart() {
        let c = QvacClient::new(fake_spec(&[]), fast_cfg());
        assert_eq!(c.call("echo", json!(1), quick()).unwrap(), json!(1));
        assert_eq!(c.call("crash", json!({}), quick()).unwrap_err(), QvacError::Crashed);
        // Next call transparently restarts the sidecar.
        assert_eq!(c.call("echo", json!(2), quick()).unwrap(), json!(2));
        assert_eq!(c.generation(), 2);
        assert!(c.is_available());
    }

    #[test]
    fn unavailable_after_restart_cap() {
        let c = QvacClient::new(fake_spec(&[]), fast_cfg());
        // Initial start + 3 restarts, each crashing.
        for _ in 0..4 {
            assert_eq!(c.call("crash", json!({}), quick()).unwrap_err(), QvacError::Crashed);
        }
        assert_eq!(c.generation(), 4);
        let err = c.call("echo", json!(1), quick()).unwrap_err();
        assert!(matches!(err, QvacError::Unavailable(_)), "got {:?}", err);
        assert!(!c.is_available());
        assert!(c.unavailable_reason().unwrap().contains("disabled for this session"));
        // Stays unavailable.
        assert!(matches!(c.call("echo", json!(1), quick()), Err(QvacError::Unavailable(_))));
    }

    #[test]
    fn failing_start_hits_cap_too() {
        let c = QvacClient::new(fake_spec(&[("QVAC_FAKE_DIE_ON_START", "1")]), fast_cfg());
        let mut errors = Vec::new();
        for _ in 0..6 {
            errors.push(c.call("echo", json!(1), quick()).unwrap_err());
        }
        assert!(matches!(errors[0], QvacError::Spawn(_)), "{:?}", errors[0]);
        assert!(matches!(errors.last().unwrap(), QvacError::Unavailable(_)));
        assert!(!c.is_available());
    }

    #[test]
    fn missing_binary_is_a_spawn_error() {
        let spec = SpawnSpec { program: PathBuf::from("/definitely/not/here/qvac-sidecar"), args: vec![], env: vec![], clear_env: true };
        let c = QvacClient::new(spec, fast_cfg());
        assert!(matches!(c.call("echo", json!(1), quick()), Err(QvacError::Spawn(_))));
    }

    #[test]
    fn stop_is_not_counted_as_crash() {
        let c = QvacClient::new(fake_spec(&[]), fast_cfg());
        for _ in 0..5 {
            assert_eq!(c.call("echo", json!(1), quick()).unwrap(), json!(1));
            c.stop();
            assert!(!c.is_running());
        }
        assert!(c.is_available());
        assert_eq!(c.generation(), 5);
    }
}
