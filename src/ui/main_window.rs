//! Main window: status readouts, startup toggles and quick actions.

use std::time::Duration;

use native_windows_gui as nwg;

use super::status_view::{StatusView, TrayProcess};
use super::widgets::{
    self, Placeable, StatusRow, Toggle, filled_button, label, place_row, place_row_right, text_button,
};
use super::{BUTTON_H, GAP, LABEL_GAP, LABEL_H, MARGIN, SECTION_GAP, STATUS_H, TOGGLE_H};
use crate::action::Action;
use crate::error::Result;
use crate::platform::startup_apps::StartupStatus;
use crate::theme::Role;
use crate::ui::resources::Resources;
use crate::vanguard::Snapshot;

/// How often the status is re-read while the app runs.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

pub struct MainWindow {
    pub window: nwg::Window,
    pub poll_timer: nwg::AnimationTimer,
    autostart: Toggle,
    vanguard_startup: Toggle,
    service_status: StatusRow,
    tray_status: StatusRow,
    process_label: nwg::Label,
    enable_btn: nwg::Button,
    disable_btn: nwg::Button,
    commands_btn: nwg::Button,
    utility_label: nwg::Label,
    kill_tray_btn: nwg::Button,
    force_kill_riot_btn: nwg::Button,
    reboot_btn: nwg::Button,
    general_btn: nwg::Button,
    hide_btn: nwg::Button,
    minimize_btn: nwg::Button,
    exit_btn: nwg::Button,
}

impl MainWindow {
    /// Initial and minimum client size: exactly what `layout` needs.
    pub const MIN_CLIENT: (i32, i32) = (760, 364);

    pub fn build(res: &Resources) -> Result<Self> {
        let window = widgets::themed_window("VG Switcher", Self::MIN_CLIENT, (300, 120), true, &res.app_icon)?;
        let w = &window;

        let mut poll_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(w)
            .interval(POLL_INTERVAL)
            .max_tick(None)
            .lifetime(None)
            .active(true)
            .build(&mut poll_timer)?;

        let this = MainWindow {
            autostart: Toggle::build(w, "Start VG Switcher on logon")?,
            vanguard_startup: Toggle::build(w, "Start Riot Vanguard at logon (same on/off as Windows' Startup Apps)")?,
            service_status: StatusRow::build(w, "Current status:", &res.bold_font)?,
            tray_status: StatusRow::build(w, "Vanguard tray/app:", &res.bold_font)?,
            process_label: label(w, "Vanguard Process:", None)?,
            enable_btn: filled_button(w, "ENABLE NOW", Role::Success)?,
            disable_btn: filled_button(w, "DISABLE NOW", Role::Error)?,
            commands_btn: text_button(w, "COMMANDS SETTINGS", Role::Primary)?,
            utility_label: label(w, "Utility:", None)?,
            kill_tray_btn: filled_button(w, "KILL VANGUARD TRAY", Role::Error)?,
            force_kill_riot_btn: filled_button(w, "FORCE KILL RIOT CLIENT", Role::Error)?,
            reboot_btn: filled_button(w, "REBOOT", Role::Error)?,
            general_btn: text_button(w, "GENERAL SETTINGS", Role::Primary)?,
            hide_btn: text_button(w, "HIDE", Role::Primary)?,
            minimize_btn: text_button(w, "MINIMIZE", Role::Primary)?,
            exit_btn: filled_button(w, "EXIT", Role::Error)?,
            poll_timer,
            window,
        };
        this.layout();
        Ok(this)
    }

    /// Everything is fixed-height; the width stretches rows and pins the
    /// bottom buttons to the right.
    pub fn layout(&self) {
        #[allow(clippy::cast_possible_wrap)]
        let w = self.window.size().0 as i32;
        let content_w = (w - MARGIN * 2).max(200);
        let mut y = MARGIN;

        self.autostart.place(y, content_w);
        y += TOGGLE_H + GAP;
        self.vanguard_startup.place(y, content_w);
        y += TOGGLE_H + SECTION_GAP;

        self.service_status.place(y, content_w);
        y += STATUS_H + GAP;
        self.tray_status.place(y, content_w);
        y += STATUS_H + SECTION_GAP;

        self.process_label.place(MARGIN, y, content_w, LABEL_H);
        y += LABEL_H + LABEL_GAP;
        place_row(MARGIN, y, BUTTON_H, &[(&self.enable_btn, 110), (&self.disable_btn, 110), (&self.commands_btn, 180)]);
        y += BUTTON_H + SECTION_GAP;

        self.utility_label.place(MARGIN, y, content_w, LABEL_H);
        y += LABEL_H + LABEL_GAP;
        place_row(
            MARGIN,
            y,
            BUTTON_H,
            &[
                (&self.kill_tray_btn, 190),
                (&self.force_kill_riot_btn, 220),
                (&self.reboot_btn, 110),
                (&self.general_btn, 170),
            ],
        );
        y += BUTTON_H + SECTION_GAP;

        place_row_right(
            w - MARGIN,
            y,
            BUTTON_H,
            &[(&self.hide_btn, 80), (&self.minimize_btn, 110), (&self.exit_btn, 80)],
        );
        debug_assert!(y + BUTTON_H + MARGIN <= Self::MIN_CLIENT.1, "MIN_CLIENT is too short for the layout");
    }

    pub fn action_for(&self, handle: nwg::ControlHandle) -> Option<Action> {
        [
            (&self.autostart.switch, Action::ToggleAutostart),
            (&self.vanguard_startup.switch, Action::ToggleVanguardStartup),
            (&self.enable_btn, Action::EnableVanguard),
            (&self.disable_btn, Action::DisableVanguard),
            (&self.commands_btn, Action::ShowCommands),
            (&self.kill_tray_btn, Action::KillVanguardTray),
            (&self.force_kill_riot_btn, Action::ForceKillRiotClient),
            (&self.reboot_btn, Action::Reboot),
            (&self.general_btn, Action::ShowGeneralSettings),
            (&self.hide_btn, Action::HideMain),
            (&self.minimize_btn, Action::MinimizeMain),
            (&self.exit_btn, Action::Quit),
        ]
        .into_iter()
        .find(|(button, _)| button.handle == handle)
        .map(|(_, action)| action)
    }

    pub fn show_status(&self, s: &Snapshot) {
        self.service_status.set(s.service.label(), s.service.color());
        let tray = TrayProcess(s.tray_running);
        self.tray_status.set(tray.label(), tray.color());
        self.vanguard_startup.set_enabled(s.startup != StartupStatus::NotFound);
        self.vanguard_startup.set_on(s.startup == StartupStatus::Enabled);
    }

    pub fn show_autostart(&self, on: bool) {
        self.autostart.set_on(on);
    }

    pub fn autostart_is_on(&self) -> bool {
        self.autostart.is_on()
    }

    pub fn vanguard_startup_is_on(&self) -> bool {
        self.vanguard_startup.is_on()
    }
}
