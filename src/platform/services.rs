//! Service configuration queries through the Service Control Manager API,
//! replacing `cmd /C sc qc <name>`. That old path parsed English console
//! output and spliced a config value into a shell command line.

use std::ptr::null_mut;

use winapi::shared::winerror::{ERROR_INSUFFICIENT_BUFFER, ERROR_SERVICE_DOES_NOT_EXIST};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winsvc::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, SC_HANDLE,
    SC_MANAGER_CONNECT, SERVICE_QUERY_CONFIG,
};

use crate::error::{AppError, Result};
use crate::platform::wide;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartType {
    Boot,
    System,
    Auto,
    Demand,
    Disabled,
    Other(u32),
}

impl StartType {
    fn from_raw(raw: u32) -> Self {
        match raw {
            0 => StartType::Boot,
            1 => StartType::System,
            2 => StartType::Auto,
            3 => StartType::Demand,
            4 => StartType::Disabled,
            other => StartType::Other(other),
        }
    }
}

/// Service names are short identifiers. Anything else (spaces, `&`, `|`,
/// quotes...) is rejected up front, so a tampered config can't smuggle
/// extra syntax in even if a name ever ends up on a command line.
pub fn is_valid_service_name(name: &str) -> bool {
    (1..=256).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}

/// The configured start type of `name`, or `Ok(None)` if no such service
/// is installed.
pub fn query_start_type(name: &str) -> Result<Option<StartType>> {
    if !is_valid_service_name(name) {
        return Err(AppError::InvalidInput(format!("\"{name}\" is not a valid service name")));
    }
    unsafe {
        let scm = OpenSCManagerW(null_mut(), null_mut(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(AppError::last_os_error("opening the Service Control Manager"));
        }
        let scm = ScHandle(scm);

        let service = OpenServiceW(scm.0, wide(name).as_ptr(), SERVICE_QUERY_CONFIG);
        if service.is_null() {
            let code = GetLastError();
            return if code == ERROR_SERVICE_DOES_NOT_EXIST {
                Ok(None)
            } else {
                Err(AppError::win32(format!("opening service \"{name}\""), code))
            };
        }
        let service = ScHandle(service);

        let mut needed = 0u32;
        if QueryServiceConfigW(service.0, null_mut(), 0, &mut needed) == 0
            && GetLastError() != ERROR_INSUFFICIENT_BUFFER
        {
            return Err(AppError::last_os_error(format!("querying service \"{name}\"")));
        }
        // u64 storage keeps the struct's pointer fields aligned.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let config = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        if QueryServiceConfigW(service.0, config, needed, &mut needed) == 0 {
            return Err(AppError::last_os_error(format!("querying service \"{name}\"")));
        }
        Ok(Some(StartType::from_raw((*config).dwStartType)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_service_names() {
        assert!(is_valid_service_name("vgc"));
        assert!(is_valid_service_name("vgk"));
        assert!(is_valid_service_name("Some_Service-2.0"));
    }

    #[test]
    fn rejects_shell_syntax_and_empty() {
        for bad in ["", "vgc & calc", "vgc|x", "a b", "\"vgc\"", "vgc;x", "x>y"] {
            assert!(!is_valid_service_name(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn maps_start_types() {
        assert_eq!(StartType::from_raw(3), StartType::Demand);
        assert_eq!(StartType::from_raw(4), StartType::Disabled);
        assert_eq!(StartType::from_raw(9), StartType::Other(9));
    }

    #[test]
    fn missing_service_reads_as_none() {
        assert_eq!(query_start_type("vg_switcher_no_such_service_xyz").unwrap(), None);
    }
}
