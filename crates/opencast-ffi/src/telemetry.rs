//! File log, in-memory ring, panic hook. Local only: `~/Library/Logs/OpenCast`.

use std::cell::RefCell;
use std::ffi::CStr;
use std::io::{self, Write};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use opencast_core::telemetry;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_BYTES: u64 = 32 * 1024 * 1024;

static LEVEL: Mutex<String> = Mutex::new(String::new());
static RELOAD: Mutex<Option<Box<dyn Fn(String) + Send>>> = Mutex::new(None);
static CLEANUP: Mutex<Option<extern "C" fn()>> = Mutex::new(None);
static GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

thread_local! {
    static PART: RefCell<String> = const { RefCell::new(String::new()) };
}

fn log_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Library/Logs/OpenCast")
}

pub fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = log_dir();
        let _ = std::fs::create_dir_all(&dir);
        prune(&dir);
        install_panic_hook();
        let spec = std::env::var("OPENCAST_LOG").unwrap_or_else(|_| "info".into());
        let (filter, handle) = tracing_subscriber::reload::Layer::new(parse_filter(&spec));
        let file = tracing_appender::rolling::daily(&dir, "opencast.log");
        let (writer, guard) = tracing_appender::non_blocking(file);
        let _ = GUARD.set(guard);
        let layer = tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .with_writer(Tee(writer))
            .with_filter(filter);
        if tracing_subscriber::registry().with(layer).try_init().is_err() {
            return;
        }
        *RELOAD.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(move |spec| {
            let _ = handle.reload(parse_filter(&spec));
        }));
        set_level_label(&spec);
        tracing::info!(dir = %dir.display(), level = %spec, "log start");
    });
}

fn parse_filter(spec: &str) -> EnvFilter {
    EnvFilter::try_new(spec).unwrap_or_else(|_| EnvFilter::new("info"))
}

fn set_level_label(spec: &str) {
    *LEVEL.lock().unwrap_or_else(|e| e.into_inner()) = spec.to_string();
}

pub fn set_level(spec: &str) {
    init();
    set_level_label(spec);
    if let Some(reload) = RELOAD.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        reload(spec.to_string());
    }
}

pub(crate) fn ipc_span(op: &str, cid: &str) -> tracing::Span {
    match op {
        "search" | "listPath" | "rankActions" | "bumpUsage" | "preview" | "hidden" | "shown" | "windowTarget"
        | "windowGlyphs" | "debugSnapshot" | "settings" | "snippets" | "apps" | "accepting" => {
            tracing::trace_span!("ipc", op, cid)
        }
        _ => tracing::info_span!("ipc", op, cid),
    }
}

pub fn snapshot(limit: usize) -> String {
    let mut v = telemetry::snapshot(limit);
    v["rssMb"] = serde_json::json!(rss_mb());
    v["cpu"] = serde_json::json!(cpu_percent());
    let (keys, problems) = crate::hotkeys::status();
    v["hotkeys"] = serde_json::json!(keys);
    v["hotkeyProblems"] = serde_json::json!(problems);
    v["logDir"] = serde_json::json!(log_dir().display().to_string());
    v["level"] = serde_json::json!(LEVEL.lock().unwrap_or_else(|e| e.into_inner()).clone());
    v.to_string()
}

pub fn export() -> Result<String, String> {
    init();
    let stamp = telemetry::now_ms();
    let dir = std::env::temp_dir().join(format!("opencast-diag-{stamp}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if let Ok(rd) = std::fs::read_dir(log_dir()) {
        for ent in rd.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("opencast.log") || name == "panic.log" {
                let _ = std::fs::copy(ent.path(), dir.join(ent.file_name()));
            }
        }
    }
    let cfg_dir = crate::host::with_engine(|e| e.config_dir.clone());
    let cfg = std::fs::read_to_string(cfg_dir.join("config.toml")).unwrap_or_default();
    std::fs::write(dir.join("config.toml"), telemetry::redact_config(&cfg)).map_err(|e| e.to_string())?;
    let mut info = format!("opencast {}\n", env!("CARGO_PKG_VERSION"));
    for args in [vec!["/usr/bin/sw_vers"], vec!["/usr/bin/uname", "-a"]] {
        if let Ok(out) = std::process::Command::new(args[0]).args(&args[1..]).output() {
            info.push_str(&String::from_utf8_lossy(&out.stdout));
        }
    }
    info.push_str(&snapshot(200));
    info.push('\n');
    std::fs::write(dir.join("system.txt"), info).map_err(|e| e.to_string())?;
    let zip = std::env::temp_dir().join(format!("opencast-diag-{stamp}.zip"));
    let status = std::process::Command::new("/usr/bin/zip")
        .arg("-qr")
        .arg(&zip)
        .arg(&dir)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("zip failed".into());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(zip.display().to_string())
}

fn prune(dir: &std::path::Path) {
    let mut files = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("opencast.log") {
            continue;
        }
        let Ok(meta) = ent.metadata() else { continue };
        let modified = meta.modified().unwrap_or(SystemTime::now());
        files.push((modified, meta.len(), ent.path()));
    }
    let now = SystemTime::now();
    files.retain(|(modified, _, path)| {
        let old = now.duration_since(*modified).unwrap_or_default() > KEEP;
        if old {
            let _ = std::fs::remove_file(path);
            false
        } else {
            true
        }
    });
    files.sort_by_key(|(t, _, _)| *t);
    if files.len() <= 1 {
        return;
    }
    let mut total: u64 = files.iter().map(|(_, n, _)| *n).sum();
    for (_, n, path) in files.iter().take(files.len() - 1) {
        if total <= MAX_BYTES {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(*n);
        }
    }
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let msg = panic_message(info.payload());
        let bt = std::backtrace::Backtrace::force_capture();
        tracing::error!(%loc, %msg, backtrace = %bt, "panic");
        sync_panic_line(&format!("{loc} {msg}\n{bt}"));
        release_stay_awake();
        prev(info);
    }));
}

fn panic_message(payload: &dyn std::any::Any) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic".into()
    }
}

pub(crate) fn log_panic_payload(payload: &(dyn std::any::Any + Send)) {
    let msg = panic_message(payload);
    tracing::error!(%msg, "ipc panicked");
    sync_panic_line(&msg);
}

fn sync_panic_line(msg: &str) {
    let path = log_dir().join("panic.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "----\n{msg}");
        let _ = f.sync_all();
    }
}

pub fn release_stay_awake() {
    let cb = *CLEANUP.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cb) = cb {
        let _ = std::panic::catch_unwind(|| cb());
    }
}

#[derive(Clone)]
struct Tee(tracing_appender::non_blocking::NonBlocking);

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Ok(s) = std::str::from_utf8(buf) {
            PART.with(|p| {
                let mut b = p.borrow_mut();
                b.push_str(s);
                while let Some(i) = b.find('\n') {
                    let line: String = b.drain(..=i).collect();
                    telemetry::push_line(&line);
                }
                if b.len() > 8_192 {
                    b.clear();
                }
            });
        }
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<'a> MakeWriter<'a> for Tee {
    type Writer = Tee;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn rss_cpu() -> Option<(u64, u64)> {
    #[repr(C)]
    struct TimeVal {
        sec: i32,
        usec: i32,
    }
    #[repr(C)]
    struct Info {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: TimeVal,
        system_time: TimeVal,
        policy: i32,
        suspend_count: i32,
    }
    extern "C" {
        fn mach_task_self() -> u32;
        fn task_info(task: u32, flavor: u32, out: *mut i32, count: *mut u32) -> i32;
    }
    const MACH_TASK_BASIC_INFO: u32 = 20;
    let mut info = std::mem::MaybeUninit::<Info>::uninit();
    let mut count = (std::mem::size_of::<Info>() / std::mem::size_of::<u32>()) as u32;
    let rc = unsafe {
        task_info(
            mach_task_self(),
            MACH_TASK_BASIC_INFO,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if rc != 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let us = |t: TimeVal| {
        (t.sec as u64)
            .saturating_mul(1_000_000)
            .saturating_add(t.usec.max(0) as u64)
    };
    Some((
        info.resident_size,
        us(info.user_time).saturating_add(us(info.system_time)),
    ))
}

fn rss_mb() -> u64 {
    rss_cpu().map(|(b, _)| b / (1024 * 1024)).unwrap_or(0)
}

fn cpu_percent() -> f64 {
    static LAST: Mutex<Option<(Instant, u64)>> = Mutex::new(None);
    let Some((_, ticks)) = rss_cpu() else { return 0.0 };
    let now = Instant::now();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let pct = if let Some((t, prev)) = *last {
        let dt = now.saturating_duration_since(t).as_secs_f64();
        let du = ticks.saturating_sub(prev) as f64 / 1_000_000.0;
        if dt > 0.05 {
            (du / dt) * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    };
    *last = Some((now, ticks));
    pct
}

fn c_str(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

#[no_mangle]
pub extern "C" fn opencast_set_log_level(level: *const c_char) {
    set_level(&c_str(level));
}

#[no_mangle]
pub extern "C" fn opencast_set_panic_cleanup(cb: Option<extern "C" fn()>) {
    *CLEANUP.lock().unwrap_or_else(|e| e.into_inner()) = cb;
}

#[no_mangle]
pub extern "C" fn opencast_log(level: *const c_char, message: *const c_char) {
    init();
    let message = telemetry::redact_line(&c_str(message));
    if message.is_empty() {
        return;
    }
    match c_str(level).to_ascii_lowercase().as_str() {
        "error" => tracing::error!(target: "opencast.ui", "{message}"),
        "warn" => tracing::warn!(target: "opencast.ui", "{message}"),
        "debug" => tracing::debug!(target: "opencast.ui", "{message}"),
        "trace" => tracing::trace!(target: "opencast.ui", "{message}"),
        _ => tracing::info!(target: "opencast.ui", "{message}"),
    }
}

#[no_mangle]
pub extern "C" fn opencast_sample(name: *const c_char, elapsed_ms: u32, budget_ms: u32) {
    let name = c_str(name);
    if name.is_empty() {
        return;
    }
    telemetry::record(&name, elapsed_ms, budget_ms);
}
