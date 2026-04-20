use std::collections::VecDeque;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use serde_json;

use chrono::Local;
use once_cell::sync::Lazy;
use tauri::{AppHandle, Manager, Runtime};
use tracing_subscriber::{fmt, layer::SubscriberExt, Registry};

// Keep the non-blocking worker guard alive for the lifetime of the app
static FILE_GUARD: Lazy<Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>> =
    Lazy::new(|| Mutex::new(None));

// In-memory ring buffer of recent log lines
const MAX_LOG_LINES: usize = 20_000;
static MEMORY_LOGS: Lazy<RwLock<VecDeque<String>>> = Lazy::new(|| RwLock::new(VecDeque::new()));

// Internal writer that collects one formatted event and pushes it to MEMORY_LOGS on drop
struct MemoryWriter<'a> {
    buf: Vec<u8>,
    logs: &'a RwLock<VecDeque<String>>,
}

impl<'a> Write for MemoryWriter<'a> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

impl<'a> Drop for MemoryWriter<'a> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.logs.write() {
            let mut s = String::from_utf8_lossy(&self.buf).to_string();
            if s.ends_with('\n') { s.pop(); } // trim one trailing newline for consistency
            guard.push_back(s);
            while guard.len() > MAX_LOG_LINES {
                guard.pop_front();
            }
        }
    }
}

struct MemoryMakeWriter;
impl<'a> fmt::MakeWriter<'a> for MemoryMakeWriter {
    type Writer = MemoryWriter<'a>;
    fn make_writer(&'a self) -> Self::Writer {
        MemoryWriter { buf: Vec::new(), logs: &MEMORY_LOGS }
    }
}

fn ensure_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn resolve_log_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    let pr = app.path();
    let mut dir = pr
        .app_log_dir()
        .or_else(|_| pr.app_data_dir())
        .or_else(|_| pr.app_cache_dir())
        .unwrap_or_else(|_| std::env::temp_dir());
    dir.push("logs");
    dir
}

pub fn init_logging<R: Runtime>(app: &AppHandle<R>) {
    // Prevent double init
    static INIT_ONCE: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
    if let Ok(mut inited) = INIT_ONCE.lock() {
        if *inited { return; }
        *inited = true;
    }

    let log_dir = resolve_log_dir(app);
    let _ = fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "autosubs.log");
    let (nb_writer, guard) = tracing_appender::non_blocking(file_appender);

    if let Ok(mut g) = FILE_GUARD.lock() {
        *g = Some(guard);
    }

    // File layer (no ANSI)
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_writer(nb_writer)
        .with_target(true)
        .with_level(true)
        .compact();

    // Memory layer
    let mem_layer = fmt::layer()
        .with_ansi(false)
        .with_writer(MemoryMakeWriter)
        .with_target(true)
        .with_level(true)
        .compact();

    // Stdout layer — live output in the console window (ANSI colours on Windows 10+)
    let stdout_layer = fmt::layer()
        .with_ansi(true)
        .with_writer(std::io::stdout)
        .with_target(true)
        .with_level(true)
        .compact();

    let subscriber = Registry::default()
        .with(file_layer)
        .with(mem_layer)
        .with(stdout_layer);
    let _ = tracing::subscriber::set_global_default(subscriber);

    tracing::info!(target: "autosubs", path = %log_dir.display(), "logging initialized");
}

// ---------------------------------------------------------------------------
// Per-job work log
// ---------------------------------------------------------------------------

/// Records the step-by-step progress of a single task (e.g. a transcription
/// job) and writes a human-readable summary to `{log_dir}/jobs/` when done.
///
/// Steps are stamped with elapsed seconds from job start.  On `finish()` or
/// `fail()` the log is flushed to disk.  If the value is dropped without
/// either call (e.g. an early `?` propagation), a `_incomplete` log is
/// written automatically.
pub struct JobLog {
    log_dir: PathBuf,
    start: std::time::Instant,
    lines: Vec<String>,
    job_name: String,
    done: bool,
}

impl JobLog {
    pub fn new(log_dir: PathBuf, job_name: impl Into<String>) -> Self {
        let job_name = job_name.into();
        let start_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut log = Self {
            log_dir,
            start: std::time::Instant::now(),
            lines: Vec::new(),
            job_name: job_name.clone(),
            done: false,
        };
        log.lines.push(format!("=== {} ===", job_name));
        log.lines.push(format!("Started:  {}", start_time));
        log.lines.push(String::new());
        tracing::info!(target: "autosubs", "Job started: {}", job_name);
        log
    }

    /// Record a named pipeline step.  The detail string is free-form.
    pub fn step(&mut self, name: &str, detail: &str) {
        let elapsed = self.start.elapsed().as_secs_f64();
        let line = format!("[{:>8.3}s] {}: {}", elapsed, name, detail);
        tracing::info!(target: "autosubs", "{}", line);
        self.lines.push(line);
    }

    /// Finalize with a success summary and flush to disk.
    pub fn finish(mut self, summary: &str) {
        let elapsed = self.start.elapsed().as_secs_f64();
        self.lines.push(String::new());
        self.lines.push(format!("=== Complete ({:.2}s) ===", elapsed));
        self.lines.push(summary.to_string());
        tracing::info!(target: "autosubs", "Job '{}' complete in {:.2}s: {}", self.job_name, elapsed, summary);
        self.done = true;
        self.flush("job");
    }

    /// Finalize with an error description and flush to disk.
    pub fn fail(mut self, error: &str) {
        let elapsed = self.start.elapsed().as_secs_f64();
        self.lines.push(String::new());
        self.lines.push(format!("=== FAILED ({:.2}s) ===", elapsed));
        self.lines.push(format!("Error: {}", error));
        tracing::warn!(target: "autosubs", "Job '{}' failed after {:.2}s: {}", self.job_name, elapsed, error);
        self.done = true;
        self.flush("job_error");
    }

    fn flush(&self, prefix: &str) {
        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
        let jobs_dir = self.log_dir.join("jobs");
        if let Err(e) = fs::create_dir_all(&jobs_dir) {
            tracing::warn!("Could not create jobs log dir {:?}: {}", jobs_dir, e);
            return;
        }
        let filename = format!("{}_{}.log", prefix, timestamp);
        let path = jobs_dir.join(&filename);
        let content = self.lines.join("\n") + "\n";
        match fs::write(&path, &content) {
            Ok(()) => tracing::info!(target: "autosubs", "Job log saved: {}", path.display()),
            Err(e) => tracing::warn!("Failed to write job log {:?}: {}", path, e),
        }
    }
}

impl Drop for JobLog {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        // Dropped without finish/fail — write an incomplete marker so it is
        // still findable in the logs directory.
        let elapsed = self.start.elapsed().as_secs_f64();
        self.lines.push(String::new());
        self.lines.push(format!("=== INCOMPLETE ({:.2}s) ===", elapsed));
        tracing::warn!(target: "autosubs", "Job '{}' dropped without completion after {:.2}s", self.job_name, elapsed);
        self.flush("job_incomplete");
    }
}

/// Convenience: create a `JobLog` whose files land in the app log directory.
pub fn new_job_log<R: Runtime>(app: &AppHandle<R>, name: impl Into<String>) -> JobLog {
    let log_dir = resolve_log_dir(app);
    let _ = fs::create_dir_all(&log_dir);
    JobLog::new(log_dir, name)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_backend_logs() -> String {
    if let Ok(guard) = MEMORY_LOGS.read() {
        guard.iter().cloned().collect::<Vec<_>>().join("\n")
    } else {
        String::new()
    }
}

#[tauri::command]
pub fn clear_backend_logs() {
    if let Ok(mut guard) = MEMORY_LOGS.write() { guard.clear(); }
}

#[tauri::command]
pub fn get_log_dir<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let dir = resolve_log_dir(&app);
    ensure_dir(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn export_backend_logs<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    // Ensure log directory exists
    let dir = resolve_log_dir(&app);
    ensure_dir(&dir).map_err(|e| e.to_string())?;

    // Collect logs from in-memory ring buffer
    let content = if let Ok(guard) = MEMORY_LOGS.read() {
        guard.iter().cloned().collect::<Vec<_>>().join("\n")
    } else {
        String::new()
    };

    // Write to a deterministic filename so users can find it easily
    let out_path = dir.join("autosubs-logs.txt");
    fs::write(&out_path, content).map_err(|e| e.to_string())?;

    Ok(out_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_lua_log() -> String {
    #[cfg(target_os = "windows")]
    {
        let temp = match std::env::var("TEMP").or_else(|_| std::env::var("TMP")) {
            Ok(t) => t,
            Err(_) => return "TEMP environment variable not set.".to_string(),
        };
        let log_path = std::path::Path::new(&temp).join("MarksAmazingSubs_launch.log");
        match fs::read_to_string(&log_path) {
            Ok(content) => content,
            Err(_) => format!(
                "Lua log not found at: {}\\MarksAmazingSubs_launch.log\nRun the 'Marks Amazing Subtitles' script in DaVinci Resolve (Workspace → Scripts) first.",
                temp
            ),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let log_path = std::path::Path::new("/tmp/MarksAmazingSubs_launch.log");
        match fs::read_to_string(log_path) {
            Ok(content) => content,
            Err(_) => "Lua log not found at /tmp/MarksAmazingSubs_launch.log.\nRun the 'Marks Amazing Subtitles' script in DaVinci Resolve (Workspace → Scripts) first.".to_string(),
        }
    }
}

#[tauri::command]
pub fn get_app_diagnostics<R: Runtime>(app: AppHandle<R>) -> serde_json::Value {
    let log_dir = resolve_log_dir(&app);
    let version = app.package_info().version.to_string();
    serde_json::json!({
        "app_version": version,
        "log_dir": log_dir.to_string_lossy().to_string(),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}
