//! The app-wide error type. Keeps the failure kind (I/O, Win32 code, bad
//! input, failed external command) instead of flattening everything to a
//! `String`, so callers can branch on it and logs stay precise.

use thiserror::Error;
use winapi::um::errhandlingapi::GetLastError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{context}: {}", describe_win32(.code))]
    Win32 { context: String, code: u32 },

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    InvalidInput(String),

    #[error("{0}")]
    CommandFailed(String),

    #[error("{0}")]
    Ui(String),
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

fn describe_win32(code: &u32) -> String {
    // io::Error already knows how to ask FormatMessageW for the text.
    #[allow(clippy::cast_possible_wrap)]
    std::io::Error::from_raw_os_error(*code as i32).to_string()
}

impl AppError {
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        AppError::Io { context: context.into(), source }
    }

    pub fn win32(context: impl Into<String>, code: u32) -> Self {
        AppError::Win32 { context: context.into(), code }
    }

    /// Wraps the calling thread's `GetLastError()` value.
    pub fn last_os_error(context: impl Into<String>) -> Self {
        let code = unsafe { GetLastError() };
        AppError::win32(context, code)
    }
}

impl From<native_windows_gui::NwgError> for AppError {
    fn from(e: native_windows_gui::NwgError) -> Self {
        AppError::Ui(e.to_string())
    }
}
