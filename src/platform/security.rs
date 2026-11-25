//! Keeps the app's data folder admin-only. The app runs elevated and
//! executes command lines from its config, so a config any user-level
//! program could edit would hand those programs administrator rights.

use std::path::Path;
use std::ptr::null_mut;

use winapi::shared::minwindef::FALSE;
use winapi::shared::sddl::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::accctrl::SE_FILE_OBJECT;
use winapi::um::aclapi::{GetNamedSecurityInfoW, SetNamedSecurityInfoW};
use winapi::um::securitybaseapi::{GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, IsWellKnownSid};
use winapi::um::winbase::LocalFree;
use winapi::um::winnt::{
    DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PACL, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, WinBuiltinAdministratorsSid, WinLocalSystemSid,
};

use crate::error::{AppError, Result};
use crate::platform::wide;

const SDDL_REVISION_1: u32 = 1;

/// Owner Administrators; SYSTEM and Administrators full control; Users read
/// and execute only. Protected, so nothing is inherited from ProgramData
/// (which lets every user create files).
const DIR_SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)";
const FILE_SDDL: &str = "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x1200a9;;;BU)";

/// A `LocalAlloc`ed security descriptor, freed on drop.
struct LocalSd(PSECURITY_DESCRIPTOR);

impl Drop for LocalSd {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

fn apply_sddl(path: &Path, sddl: &str) -> Result<()> {
    let context = || format!("securing {}", path.display());
    unsafe {
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(sddl).as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            null_mut(),
        ) == 0
        {
            return Err(AppError::last_os_error(context()));
        }
        let sd = LocalSd(sd);

        let mut owner: PSID = null_mut();
        let mut dacl: PACL = null_mut();
        let (mut present, mut defaulted) = (FALSE, FALSE);
        if GetSecurityDescriptorOwner(sd.0, &mut owner, &mut defaulted) == 0
            || GetSecurityDescriptorDacl(sd.0, &mut present, &mut dacl, &mut defaulted) == 0
        {
            return Err(AppError::last_os_error(context()));
        }

        let mut name = wide(&path.to_string_lossy());
        let rc = SetNamedSecurityInfoW(
            name.as_mut_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            owner,
            null_mut(),
            dacl,
            null_mut(),
        );
        if rc == ERROR_SUCCESS { Ok(()) } else { Err(AppError::win32(context(), rc)) }
    }
}

/// Creates `dir` if needed and (re)applies the admin-only ACL and owner.
/// Re-applied on every start, so a folder pre-created by a standard user is
/// taken over rather than trusted.
pub fn ensure_secure_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| AppError::io(format!("creating {}", dir.display()), e))?;
    apply_sddl(dir, DIR_SDDL)
}

/// Applies the admin-only ACL and owner to a file the app just wrote.
pub fn secure_file(path: &Path) -> Result<()> {
    apply_sddl(path, FILE_SDDL)
}

/// Whether `path` is owned by Administrators or SYSTEM, i.e. was written by
/// an elevated process rather than planted by a standard user.
pub fn has_trusted_owner(path: &Path) -> Result<bool> {
    unsafe {
        let mut owner: PSID = null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        let mut name = wide(&path.to_string_lossy());
        let rc = GetNamedSecurityInfoW(
            name.as_mut_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        );
        if rc != ERROR_SUCCESS {
            return Err(AppError::win32(format!("reading the owner of {}", path.display()), rc));
        }
        let _sd = LocalSd(sd);
        Ok(IsWellKnownSid(owner, WinBuiltinAdministratorsSid) != 0 || IsWellKnownSid(owner, WinLocalSystemSid) != 0)
    }
}
