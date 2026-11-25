//! Windows, tray and the building blocks they share. Each window owns its
//! controls, its layout and its `handle -> Action` mapping. What an action
//! *does* lives in `app.rs`.

pub mod commands_window;
pub mod dialog;
pub mod main_window;
pub mod reboot_window;
pub mod resources;
pub mod settings_window;
pub mod status_view;
pub mod tray;
pub mod widgets;

use native_windows_gui as nwg;
use winapi::shared::windef::RECT;
use winapi::um::winuser::{AdjustWindowRectEx, GWL_EXSTYLE, GWL_STYLE, GetWindowLongW, SetForegroundWindow};

// Shared layout metrics (logical pixels).
pub const MARGIN: i32 = 10;
/// Height of every label: exactly one line of text. See AGENTS.md - a
/// taller nwg::Label paints white bands above and below its text.
pub const LABEL_H: i32 = 18;
pub const LABEL_GAP: i32 = 6;
pub const UNDERLINE_H: i32 = 3;
pub const GAP: i32 = 8;
pub const H_GAP: i32 = 10;
pub const SECTION_GAP: i32 = 20;
pub const BUTTON_H: i32 = 32;
pub const SERVICE_INPUT_H: i32 = 24;
pub const TOGGLE_W: i32 = 44;
pub const TOGGLE_H: i32 = 24;
pub const STATUS_H: i32 = 28;

/// Hides `window` instead of destroying it when its close (X) is clicked.
/// Returns whether the event was for `window`.
pub fn hide_on_close(window: &nwg::Window, handle: nwg::ControlHandle, data: &nwg::EventData) -> bool {
    if handle != window.handle {
        return false;
    }
    if let nwg::EventData::OnWindowClose(close) = data {
        close.close(false);
    }
    window.set_visible(false);
    true
}

/// Answers `OnMinMaxInfo` so the window's *client area* can't shrink below
/// `min_client`. (nwg sizes windows by client area, but the min-size hook
/// takes outer sizes, so the frame is added here.)
pub fn enforce_min_client_size(window: &nwg::Window, data: &nwg::EventData, min_client: (i32, i32)) {
    let (nwg::EventData::OnMinMaxInfo(info), Some(hwnd)) = (data, window.handle.hwnd()) else {
        return;
    };
    let mut rect = RECT { left: 0, top: 0, right: min_client.0, bottom: min_client.1 };
    #[allow(clippy::cast_sign_loss)]
    unsafe {
        AdjustWindowRectEx(
            &mut rect,
            GetWindowLongW(hwnd, GWL_STYLE) as u32,
            0,
            GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
        );
    }
    info.set_min_size(rect.right - rect.left, rect.bottom - rect.top);
}

/// Shows a window and brings it to the front.
pub fn present(window: &nwg::Window) {
    window.set_visible(true);
    window.restore();
    if let Some(hwnd) = window.handle.hwnd() {
        // set_focus alone doesn't raise a window behind other apps (e.g.
        // when opened from the tray or by a second launch).
        unsafe {
            SetForegroundWindow(hwnd);
        }
    }
    window.set_focus();
}
