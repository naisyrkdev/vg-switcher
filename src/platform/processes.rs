//! Finding and terminating processes by image name through the ToolHelp
//! snapshot API, replacing `tasklist` / `taskkill` and their localized
//! console output.

use std::mem::size_of;

use winapi::shared::winerror::ERROR_INVALID_PARAMETER;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
use winapi::um::tlhelp32::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use winapi::um::winnt::PROCESS_TERMINATE;

use crate::error::{AppError, Result};
use crate::platform::{OwnedHandle, from_wide};

/// IDs of every running process whose image name equals `image_name`
/// (case-insensitive, e.g. `"vgtray.exe"`).
pub fn find(image_name: &str) -> Result<Vec<u32>> {
    unsafe {
        let snapshot = OwnedHandle::new(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0))
            .ok_or_else(|| AppError::last_os_error("listing running processes"))?;
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        #[allow(clippy::cast_possible_truncation)]
        {
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        }
        let mut pids = Vec::new();
        let mut more = Process32FirstW(snapshot.raw(), &mut entry) != 0;
        while more {
            if from_wide(&entry.szExeFile).eq_ignore_ascii_case(image_name) {
                pids.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot.raw(), &mut entry) != 0;
        }
        Ok(pids)
    }
}

pub fn is_running(image_name: &str) -> bool {
    match find(image_name) {
        Ok(pids) => !pids.is_empty(),
        Err(e) => {
            log::warn!("could not check whether {image_name} is running: {e}");
            false
        }
    }
}

/// Force-terminates every process named `image_name`. Nothing running is
/// not an error. Returns how many processes were terminated.
pub fn kill_all(image_name: &str) -> Result<usize> {
    let mut killed = 0;
    let mut errors = Vec::new();
    for pid in find(image_name)? {
        match terminate(pid) {
            Ok(true) => killed += 1,
            Ok(false) => {} // exited on its own in the meantime
            Err(e) => errors.push(format!("PID {pid}: {e}")),
        }
    }
    if errors.is_empty() {
        Ok(killed)
    } else {
        Err(AppError::CommandFailed(format!("Could not terminate {image_name}:\n{}", errors.join("\n"))))
    }
}

/// `Ok(false)` if the process was already gone.
fn terminate(pid: u32) -> Result<bool> {
    unsafe {
        let Some(process) = OwnedHandle::new(OpenProcess(PROCESS_TERMINATE, 0, pid)) else {
            let code = GetLastError();
            return if code == ERROR_INVALID_PARAMETER {
                Ok(false)
            } else {
                Err(AppError::win32("opening the process", code))
            };
        };
        if TerminateProcess(process.raw(), 1) == 0 {
            return Err(AppError::last_os_error("terminating the process"));
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_process_that_is_always_running() {
        assert!(is_running("System") || is_running("explorer.exe") || is_running("svchost.exe"));
    }

    #[test]
    fn missing_process_is_not_running_and_kill_is_a_no_op() {
        assert!(!is_running("vg_switcher_no_such_process.exe"));
        assert_eq!(kill_all("vg_switcher_no_such_process.exe").unwrap(), 0);
    }
}
