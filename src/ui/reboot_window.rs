//! Shown after enabling Vanguard from a disabled state: the new service
//! start types only take effect after a reboot.

use native_windows_gui as nwg;

use super::widgets::{self, Placeable, filled_button, label, place_row, text_button};
use super::{BUTTON_H, LABEL_GAP, LABEL_H, MARGIN};
use crate::action::Action;
use crate::error::Result;
use crate::theme::Role;
use crate::ui::resources::Resources;

pub struct RebootWindow {
    pub window: nwg::Window,
    // Never read, but owning them keeps the controls alive.
    _message: [nwg::Label; 2],
    reboot_now_btn: nwg::Button,
    later_btn: nwg::Button,
}

impl RebootWindow {
    pub fn build(res: &Resources) -> Result<Self> {
        let window =
            widgets::themed_window("VG Switcher - Reboot required", (460, 150), (400, 220), false, &res.app_icon)?;

        // One label per line, each exactly LABEL_H tall (see AGENTS.md).
        let message = [
            label(&window, "Vanguard has been enabled.", None)?,
            label(&window, "To make it take effect you need to reboot your computer.", None)?,
        ];
        let mut y = MARGIN + 10;
        for line in &message {
            line.place(MARGIN, y, 440, LABEL_H);
            y += LABEL_H + LABEL_GAP;
        }

        let reboot_now_btn = filled_button(&window, "REBOOT NOW", Role::Error)?;
        let later_btn = text_button(&window, "REBOOT LATER (CLOSE THIS WINDOW)", Role::Primary)?;
        place_row(MARGIN, 100, BUTTON_H, &[(&reboot_now_btn, 130), (&later_btn, 300)]);

        Ok(RebootWindow { window, _message: message, reboot_now_btn, later_btn })
    }

    pub fn action_for(&self, handle: nwg::ControlHandle) -> Option<Action> {
        if handle == self.reboot_now_btn.handle {
            Some(Action::RebootFromPrompt)
        } else if handle == self.later_btn.handle {
            Some(Action::DismissRebootPrompt)
        } else {
            None
        }
    }
}
