//! Everything the user can ask the app to do. Buttons, tray menu items and
//! window messages all map to an `Action`, and `App::dispatch` is the one
//! place that carries actions out.

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    EnableVanguard,
    DisableVanguard,
    KillVanguardTray,
    ForceKillRiotClient,
    Reboot,
    ToggleAutostart,
    ToggleVanguardStartup,

    ShowMain,
    HideMain,
    MinimizeMain,
    ShowCommands,
    HideCommands,
    ShowGeneralSettings,
    HideGeneralSettings,
    SetTheme(Theme),

    RebootFromPrompt,
    DismissRebootPrompt,

    /// An Edit / Save / Cancel / Restore click on one of the Commands
    /// Settings fields (index into `CommandsWindow::fields`).
    Field(usize, FieldOp),

    Refresh,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOp {
    Edit,
    Save,
    Cancel,
    RestoreDefault,
}
