//! Win32 side of theming. Windows has no public dark-mode API for classic
//! controls; this is the combination every dark Win32 app uses:
//!
//! 1. uxtheme ordinal 135 `SetPreferredAppMode` (undocumented, Windows 10
//!    1809+) opts the process into dark-capable controls.
//! 2. `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)` themes title bars.
//! 3. uxtheme ordinal 133 `AllowDarkModeForWindow` + `SetWindowTheme(
//!    "DarkMode_Explorer")` switches a control to its dark visual style.
//! 4. Edit/Static controls have no dark style, so the parent window is
//!    subclassed to answer `WM_CTLCOLOR*` / `WM_ERASEBKGND` itself.
//!
//! The ordinals are only used on builds that are known to have them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::c_int;
use std::mem::{size_of, transmute};
use std::ptr::null_mut;
use std::sync::OnceLock;

use winapi::shared::minwindef::{BOOL, DWORD, FALSE, LPARAM, LRESULT, TRUE, UINT, WPARAM};
use winapi::shared::windef::{HBRUSH, HDC, HWND, RECT};
use winapi::um::commctrl::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use winapi::um::dwmapi::DwmSetWindowAttribute;
use winapi::um::libloaderapi::{GetProcAddress, LoadLibraryW};
use winapi::um::uxtheme::SetWindowTheme;
use winapi::um::wingdi::{CreateSolidBrush, DeleteObject, OPAQUE, SetBkColor, SetBkMode, SetTextColor};
use winapi::um::winuser::{
    DRAWITEMSTRUCT, FillRect, GetClientRect, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW,
    RedrawWindow, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, WM_CTLCOLORBTN,
    WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DRAWITEM, WM_ERASEBKGND, WM_MOUSELEAVE, WM_MOUSEMOVE,
    WM_NCDESTROY,
};

use super::{Rgb, colorref, control_bg, controls, dark_chrome, text_fg, window_bg};
use crate::platform::registry::{self, Hive};
use crate::platform::wide;

const DWMWA_USE_IMMERSIVE_DARK_MODE: DWORD = 20;
const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: DWORD = 19; // pre-20H1 builds

/// Windows 10 1809, the first build where ordinals 133/135 have the
/// signatures used here.
const MIN_DARK_MODE_BUILD: u32 = 17763;

// Distinctive ids so we don't collide with nwg's own subclass ids.
const BACKGROUND_SUBCLASS_ID: usize = 0x5647_5F44; // 'VG_D'
const CONTROL_SUBCLASS_ID: usize = 0x5647_5F48; // 'VG_H'

type SetPreferredAppMode = unsafe extern "system" fn(c_int) -> c_int;
type AllowDarkModeForWindow = unsafe extern "system" fn(HWND, BOOL) -> BOOL;

struct UxTheme {
    set_preferred_app_mode: Option<SetPreferredAppMode>,
    allow_dark_mode_for_window: Option<AllowDarkModeForWindow>,
}

fn windows_build() -> Option<u32> {
    registry::read_string(Hive::LocalMachine, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber")
        .ok()
        .flatten()
        .and_then(|s| s.trim().parse().ok())
}

/// Resolves the undocumented uxtheme exports once.
fn uxtheme() -> &'static UxTheme {
    static UXTHEME: OnceLock<UxTheme> = OnceLock::new();
    UXTHEME.get_or_init(|| {
        let none = UxTheme { set_preferred_app_mode: None, allow_dark_mode_for_window: None };
        if windows_build().is_none_or(|b| b < MIN_DARK_MODE_BUILD) {
            log::info!("Windows build too old for dark-mode ordinals; using light control styles");
            return none;
        }
        unsafe {
            let lib = LoadLibraryW(wide("uxtheme.dll").as_ptr());
            if lib.is_null() {
                return none;
            }
            let ordinal = |n: usize| GetProcAddress(lib, n as *const i8);
            let (p135, p133) = (ordinal(135), ordinal(133));
            UxTheme {
                set_preferred_app_mode: (!p135.is_null()).then(|| transmute::<_, SetPreferredAppMode>(p135)),
                allow_dark_mode_for_window: (!p133.is_null()).then(|| transmute::<_, AllowDarkModeForWindow>(p133)),
            }
        }
    })
}

thread_local! {
    static THEMED_WINDOWS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static THEMED_CONTROLS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static BRUSHES: RefCell<HashMap<Rgb, HBRUSH>> = RefCell::new(HashMap::new());
}

/// A cached solid brush. Cleared (and the brushes deleted) on theme change.
pub(super) fn brush(color: Rgb) -> HBRUSH {
    BRUSHES.with(|m| *m.borrow_mut().entry(color).or_insert_with(|| unsafe { CreateSolidBrush(colorref(color)) }))
}

fn clear_brushes() {
    BRUSHES.with(|m| {
        for (_, b) in m.borrow_mut().drain() {
            unsafe {
                DeleteObject(b.cast());
            }
        }
    });
}

/// Opts the whole process into dark-capable common controls. Call once,
/// before creating any windows.
pub fn enable_app_dark_mode() {
    if let Some(f) = uxtheme().set_preferred_app_mode {
        unsafe {
            f(1); // AllowDark
        }
    }
}

/// Themes a top-level window's title bar to match the current theme.
pub fn apply_titlebar_theme(hwnd: HWND) {
    let value: BOOL = if dark_chrome() { TRUE } else { FALSE };
    let set =
        |attr| unsafe { DwmSetWindowAttribute(hwnd, attr, (&raw const value).cast(), size_of::<BOOL>() as DWORD) };
    if set(DWMWA_USE_IMMERSIVE_DARK_MODE) != 0 {
        set(DWMWA_USE_IMMERSIVE_DARK_MODE_OLD);
    }
    // Redraw the frame now rather than on the next resize.
    unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn set_dark_visual_style(hwnd: HWND, dark: bool) {
    unsafe {
        if let Some(f) = uxtheme().allow_dark_mode_for_window {
            f(hwnd, BOOL::from(dark));
        }
        if dark && uxtheme().allow_dark_mode_for_window.is_some() {
            SetWindowTheme(hwnd, wide("DarkMode_Explorer").as_ptr(), null_mut());
        } else {
            SetWindowTheme(hwnd, null_mut(), null_mut());
        }
    }
}

/// Gives a native (not owner-drawn) control, e.g. a text field, the current
/// theme's visual style, and keeps it in sync on theme switches. Without it
/// newer Windows paints a light background under single-line Edits.
pub fn apply_themed_control(hwnd: HWND) {
    THEMED_CONTROLS.with(|s| s.borrow_mut().insert(hwnd as usize));
    install_control_subclass(hwnd);
    set_dark_visual_style(hwnd, dark_chrome());
}

/// Installs the themed background/`WM_CTLCOLOR*` handling on a top-level
/// window, and registers it for theme-switch repaints.
pub fn install_themed_background(hwnd: HWND) {
    THEMED_WINDOWS.with(|s| s.borrow_mut().insert(hwnd as usize));
    unsafe {
        SetWindowSubclass(hwnd, Some(background_proc), BACKGROUND_SUBCLASS_ID, 0);
    }
}

/// Re-applies the current theme to everything registered.
pub(super) fn repaint_for_theme_change() {
    clear_brushes();
    let dark = dark_chrome();
    let controls: Vec<usize> = THEMED_CONTROLS.with(|s| s.borrow().iter().copied().collect());
    for hwnd in controls {
        set_dark_visual_style(hwnd as HWND, dark);
    }
    let windows: Vec<usize> = THEMED_WINDOWS.with(|s| s.borrow().iter().copied().collect());
    for hwnd in windows {
        let hwnd = hwnd as HWND;
        apply_titlebar_theme(hwnd);
        unsafe {
            RedrawWindow(
                hwnd,
                null_mut(),
                null_mut(),
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_FRAME,
            );
        }
    }
}

/// Subclass for individual controls: hover tracking for owner-drawn
/// buttons, and dropping the control's registry entries when it's destroyed.
pub(super) fn install_control_subclass(hwnd: HWND) {
    unsafe {
        SetWindowSubclass(hwnd, Some(control_proc), CONTROL_SUBCLASS_ID, 0);
    }
}

unsafe extern "system" fn control_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => controls::on_mouse_move(hwnd),
        WM_MOUSELEAVE => controls::on_mouse_leave(hwnd),
        WM_NCDESTROY => {
            controls::forget(hwnd);
            THEMED_CONTROLS.with(|s| s.borrow_mut().remove(&(hwnd as usize)));
            unsafe {
                RemoveWindowSubclass(hwnd, Some(control_proc), CONTROL_SUBCLASS_ID);
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn background_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let hdc = wparam as HDC;
                SetTextColor(hdc, colorref(text_fg()));
                SetBkColor(hdc, colorref(control_bg()));
                brush(control_bg()) as LRESULT
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let hdc = wparam as HDC;
                let text = controls::static_text_color(lparam as HWND).unwrap_or_else(text_fg);
                SetTextColor(hdc, colorref(text));
                SetBkColor(hdc, colorref(window_bg()));
                // OPAQUE, not TRANSPARENT: with ClearType a transparent
                // background blends glyph edges against whatever stale pixels
                // are underneath, which can make text near-invisible. OPAQUE
                // repaints each glyph cell in window_bg() first.
                SetBkMode(hdc, OPAQUE as i32);
                brush(window_bg()) as LRESULT
            }
            WM_DRAWITEM => {
                if controls::draw_item(&*(lparam as *const DRAWITEMSTRUCT)) {
                    1
                } else {
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
            }
            WM_ERASEBKGND => {
                let hdc = wparam as HDC;
                let mut rect: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rect);
                FillRect(hdc, &rect, brush(window_bg()));
                controls::paint_accent_bars(hdc, hwnd);
                1
            }
            WM_NCDESTROY => {
                THEMED_WINDOWS.with(|s| s.borrow_mut().remove(&(hwnd as usize)));
                controls::forget_owner(hwnd);
                RemoveWindowSubclass(hwnd, Some(background_proc), BACKGROUND_SUBCLASS_ID);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
}
