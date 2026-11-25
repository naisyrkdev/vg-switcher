//! Running external programs without a console window. Only used where no
//! reasonable API exists (Task Scheduler, the shutdown dialog) and for the
//! user's own enable/disable command lists.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::error::{AppError, Result};
use crate::platform::paths;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug)]
pub struct CmdOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// stdout and stderr together, trimmed - for error messages.
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.stdout.trim(), self.stderr.trim()).trim().to_string()
    }
}

/// Runs `program` with `args`. Pass an absolute path (see
/// [`paths::system_tool`]): a bare name is also looked up in the exe's own
/// folder, which isn't safe for an elevated process.
pub fn run(program: &Path, args: &[&str]) -> Result<CmdOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    exec(cmd, &program.display().to_string())
}

/// Runs one command line through `cmd.exe /C`, passed through verbatim
/// (`raw_arg`), exactly as if it were typed at a prompt.
pub fn run_shell_line(line: &str) -> Result<CmdOutput> {
    let mut cmd = Command::new(paths::system_tool("cmd.exe"));
    cmd.arg("/C").raw_arg(line);
    exec(cmd, line)
}

fn exec(mut cmd: Command, what: &str) -> Result<CmdOutput> {
    let output = cmd
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| AppError::io(format!("failed to run `{what}`"), e))?;
    Ok(CmdOutput {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
