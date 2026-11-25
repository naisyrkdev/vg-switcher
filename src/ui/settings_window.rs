//! "General Settings": app-level preferences (currently the colour theme).

use native_windows_gui as nwg;

use super::widgets::{self, Placeable, label, text_button};
use super::{BUTTON_H, LABEL_H, MARGIN};
use crate::action::Action;
use crate::error::Result;
use crate::theme::{Role, Theme};
use crate::ui::resources::Resources;

const CONTENT_W: i32 = 320;
const THEME_BUTTON_H: i32 = 34;
const THEME_BUTTON_PITCH: i32 = 42;

pub struct SettingsWindow {
    pub window: nwg::Window,
    // Never read, but owning it keeps the control alive.
    _title: nwg::Label,
    current_theme: nwg::Label,
    theme_buttons: Vec<(nwg::Button, Theme)>,
    close_btn: nwg::Button,
}

impl SettingsWindow {
    pub fn build(res: &Resources) -> Result<Self> {
        let window =
            widgets::themed_window("VG Switcher - General Settings", (360, 260), (360, 160), false, &res.app_icon)?;

        let title = label(&window, "Theme:", None)?;
        title.place(MARGIN, MARGIN, CONTENT_W, LABEL_H);
        let current_theme = label(&window, "", Some(&res.bold_font))?;
        current_theme.place(MARGIN, 34, CONTENT_W, LABEL_H);

        let mut theme_buttons = Vec::new();
        let mut y = 66;
        for theme in Theme::ALL {
            let button = text_button(&window, theme.button_text(), Role::Primary)?;
            button.place(MARGIN, y, CONTENT_W, THEME_BUTTON_H);
            theme_buttons.push((button, theme));
            y += THEME_BUTTON_PITCH;
        }

        let close_btn = text_button(&window, "CLOSE", Role::Primary)?;
        close_btn.place(MARGIN + CONTENT_W - 80, y + 12, 80, BUTTON_H);

        Ok(SettingsWindow { window, _title: title, current_theme, theme_buttons, close_btn })
    }

    pub fn show_theme(&self, theme: Theme) {
        self.current_theme.set_text(&format!("Current theme: {}", theme.label()));
    }

    pub fn action_for(&self, handle: nwg::ControlHandle) -> Option<Action> {
        if handle == self.close_btn.handle {
            return Some(Action::HideGeneralSettings);
        }
        self.theme_buttons.iter().find(|(b, _)| b.handle == handle).map(|(_, t)| Action::SetTheme(*t))
    }
}
