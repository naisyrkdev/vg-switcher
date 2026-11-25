//! A small file logger (`%ProgramData%\VG_Switcher\vg_switcher.log`) and a
//! panic hook. The app has no console, so without these a failure or crash
//! leaves no trace.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::ptr::null_mut;
use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};
use winapi::um::minwinbase::SYSTEMTIME;
use winapi::um::sysinfoapi::GetLocalTime;
use winapi::um::winuser::{MB_ICONERROR, MB_OK, MessageBoxW};

use crate::platform::wide;

const FILE_NAME: &str = "vg_switcher.log";
const MAX_LOG_BYTES: u64 = 1024 * 1024;

struct FileLogger {
    file: Mutex<File>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{} {:<5} {}: {}", timestamp(), record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

fn timestamp() -> String {
    let mut t: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

/// Starts logging to `dir`. Keeps one previous file (`.log.old`) once the
/// log passes 1 MiB. Logging is best-effort: failures here are ignored.
pub fn init(dir: &Path) {
    let path = dir.join(FILE_NAME);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let level = if cfg!(debug_assertions) { LevelFilter::Debug } else { LevelFilter::Info };
    if log::set_boxed_logger(Box::new(FileLogger { file: Mutex::new(file) })).is_ok() {
        log::set_max_level(level);
    }
}

/// Logs any panic with a backtrace and tells the user before the process
/// ends (release builds abort on panic).
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!("panic: {info}\n{backtrace}");
        log::logger().flush();
        let text = format!(
            "VG Switcher ran into an unexpected error and has to close.\n\n{info}\n\n\
             Details were written to %ProgramData%\\VG_Switcher\\{FILE_NAME}."
        );
        unsafe {
            MessageBoxW(null_mut(), wide(&text).as_ptr(), wide("VG Switcher").as_ptr(), MB_OK | MB_ICONERROR);
        }
    }));
}
