//! The notification-area icon and its menu.
//!
//! tray-icon reports clicks through global handlers. Instead of polling its
//! channels on a timer, the handlers forward each event into our own
//! channel and post a message to the main window, which then drains the
//! channel on the UI thread.

use std::cell::Cell;
use std::sync::mpsc::Sender;

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winapi::shared::windef::HWND;
use winapi::um::winuser::{PostMessageW, WM_APP};

use super::status_view::{StatusView, tray_text};
use crate::action::Action;
use crate::error::{AppError, Result};
use crate::theme::{self, Theme};
use crate::vanguard::VanguardStatus;

/// Posted to the main window when tray input is waiting in the channel.
pub const WM_TRAY_INPUT: u32 = WM_APP + 1;

#[derive(Debug, Clone)]
pub enum TrayInput {
    Menu(MenuId),
    DoubleClick,
}

pub struct Tray {
    icon: TrayIcon,
    status_item: MenuItem,
    actions: Vec<(MenuItem, Action)>,
    /// What the icon currently shows, to skip rebuilding it needlessly.
    shown: Cell<Option<(VanguardStatus, Theme)>>,
}

impl Tray {
    pub fn build() -> Result<Self> {
        let ui_err = |e: &dyn std::fmt::Display| AppError::Ui(format!("creating the tray icon: {e}"));

        let status_item = MenuItem::new(tray_text(VanguardStatus::Unknown), false, None);
        let item = |text: &str, action| (MenuItem::new(text, true, None), action);
        let actions = vec![
            item("Enable Vanguard", Action::EnableVanguard),
            item("Disable Vanguard", Action::DisableVanguard),
            item("Kill Vanguard Tray", Action::KillVanguardTray),
            item("Open VG Switcher", Action::ShowMain),
            item("General Settings...", Action::ShowGeneralSettings),
            item("Refresh", Action::Refresh),
            item("Exit", Action::Quit),
        ];
        let sep = PredefinedMenuItem::separator();
        let a = |i: usize| &actions[i].0;

        let menu = Menu::new();
        menu.append_items(&[&status_item, &sep, a(0), a(1), a(2), &sep, a(3), a(4), a(5), &sep, a(6)])
            .map_err(|e| ui_err(&e))?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("VG Switcher")
            .with_icon(status_icon(VanguardStatus::Unknown)?)
            .build()
            .map_err(|e| ui_err(&e))?;

        Ok(Tray { icon, status_item, actions, shown: Cell::new(None) })
    }

    pub fn action_for(&self, id: &MenuId) -> Option<Action> {
        self.actions.iter().find(|(item, _)| item.id() == id).map(|(_, a)| *a)
    }

    /// Updates icon, tooltip and status line if the status or theme changed.
    pub fn show_status(&self, status: VanguardStatus) {
        let key = (status, theme::current_theme());
        if self.shown.get() == Some(key) {
            return;
        }
        self.shown.set(Some(key));
        let text = tray_text(status);
        self.status_item.set_text(&text);
        let _ = self.icon.set_tooltip(Some(&text));
        match status_icon(status) {
            Ok(icon) => {
                let _ = self.icon.set_icon(Some(icon));
            }
            Err(e) => log::warn!("{e}"),
        }
    }
}

/// Routes tray clicks into `sender` and wakes `hwnd` with `WM_TRAY_INPUT`.
pub fn forward_events(hwnd: HWND, sender: Sender<TrayInput>) {
    let hwnd = hwnd as usize; // HWND isn't Send; the value is all we need.
    let notify = move |input: TrayInput, sender: &Sender<TrayInput>| {
        if sender.send(input).is_ok() {
            unsafe {
                PostMessageW(hwnd as HWND, WM_TRAY_INPUT, 0, 0);
            }
        }
    };
    let menu_sender = sender.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| notify(TrayInput::Menu(e.id), &menu_sender)));
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::DoubleClick { .. } = e {
            notify(TrayInput::DoubleClick, &sender);
        }
    }));
}

/// A filled circle in the status colour.
fn status_icon(status: VanguardStatus) -> Result<Icon> {
    const SIZE: u32 = 32;
    let (r, g, b) = status.color();
    let center = SIZE as f32 / 2.0;
    let radius = center - 2.0;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for (i, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let (x, y) = ((i as u32 % SIZE) as f32, (i as u32 / SIZE) as f32);
        let (dx, dy) = (x - center + 0.5, y - center + 0.5);
        if dx.hypot(dy) <= radius {
            *px = [r, g, b, 255];
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| AppError::Ui(format!("building the tray icon: {e}")))
}
