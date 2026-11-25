// Windows ships no public "dark mode" API for classic Win32 controls. Every
// well-known dark-themed Win32 app (Explorer, Notepad, VS Code's native
// frame, etc.) uses the same combination this module implements:
//   1. uxtheme.dll ordinal 135 (SetPreferredAppMode) - undocumented, stable
//      since Windows 10 1809 - opts the whole process into dark-capable
//      controls.
//   2. DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE) - documented -
//      dark-themes the window's title bar/border.
//   3. uxtheme.dll ordinal 133 (AllowDarkModeForWindow) + the documented
//      SetWindowTheme(hwnd, "DarkMode_Explorer", NULL) - switches buttons
//      and checkboxes to their dark visual-style variant.
//   4. Edit/Static controls have no dark visual-style variant at all, so we
//      subclass the parent window and manually answer WM_CTLCOLOREDIT /
//      WM_CTLCOLORSTATIC / WM_ERASEBKGND with dark brushes and light text.
//
// On top of that, buttons and toggle switches are fully owner-drawn
// (BS_OWNERDRAW) so we can render them as Material-style flat/filled controls
// - rounded corners, a "filled" vs "text" button distinction, and a
// lightened/darkened state layer on hover/press - rather than the native
// 3D/themed look. No focus rectangle is drawn on any of them.
//
// Three palettes (Theme::Dark/Light/Dracula) can be swapped at runtime via
// `set_theme`. Every color used for painting is fetched through a function
// (`window_bg()`, `primary()`, etc.) that reads the *current* theme rather
// than a baked-in constant, and owner-drawn controls store a `Role` (which
// palette slot they use) instead of a raw RGB - so a theme switch repaints
// everything in the new palette without recreating any control.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::mem::{size_of, transmute};
use std::ptr::null_mut;

use winapi::ctypes::c_int;
use winapi::shared::minwindef::{BOOL, DWORD, LPARAM, LRESULT, TRUE, UINT, WPARAM};
use winapi::shared::ntdef::LPCSTR;
use winapi::shared::windef::{HBRUSH, HDC, HFONT, HGDIOBJ, HWND, RECT, SIZE};
use winapi::um::commctrl::{DefSubclassProc, SetWindowSubclass};
use winapi::um::dwmapi::DwmSetWindowAttribute;
use winapi::um::libloaderapi::{GetProcAddress, LoadLibraryA};
use winapi::um::uxtheme::SetWindowTheme;
use winapi::um::wingdi::{
    CreatePen, CreateSolidBrush, DeleteObject, GetTextExtentPoint32W, RoundRect, SelectObject,
    SetBkColor, SetBkMode, SetTextColor, OPAQUE, PS_SOLID, TRANSPARENT,
};
use winapi::um::winuser::{
    DrawTextW, FillRect, GetClientRect, GetDC, GetWindowLongPtrW, GetWindowTextW,
    InvalidateRect, RedrawWindow, ReleaseDC, SendMessageW, SetWindowLongPtrW, SetWindowPos,
    TrackMouseEvent, DRAWITEMSTRUCT, DT_CENTER, DT_SINGLELINE, DT_VCENTER, GWL_STYLE,
    ODS_DISABLED, ODS_SELECTED, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE,
    RDW_UPDATENOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    TRACKMOUSEEVENT, TME_LEAVE, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX,
    WM_CTLCOLORSTATIC, WM_DRAWITEM, WM_ERASEBKGND, WM_GETFONT, WM_MOUSELEAVE, WM_MOUSEMOVE,
};

const BS_OWNERDRAW: i32 = 0x0000_000B;
const BS_TYPEMASK: isize = 0x0000_000F;

const DWMWA_USE_IMMERSIVE_DARK_MODE: DWORD = 20;
const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: DWORD = 19; // pre-20H1 builds

/// Corner radius used for buttons.
pub const BUTTON_RADIUS: i32 = 10;

/// The three selectable app themes. `Dark` is the original Material dark
/// palette and remains the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
    Dracula,
}

impl Theme {
    /// Stable string used for persistence in `Config`.
    pub fn key(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::Dracula => "dracula",
        }
    }

    /// Parses a `Config`-persisted key, falling back to `Dark` for anything
    /// unrecognized (e.g. a config written by an older build).
    pub fn from_key(key: &str) -> Theme {
        match key {
            "light" => Theme::Light,
            "dracula" => Theme::Dracula,
            _ => Theme::Dark,
        }
    }

    /// Human-readable label for the General Settings UI.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark (default)",
            Theme::Light => "Light",
            Theme::Dracula => "Dracula",
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    window_bg: (u8, u8, u8),
    control_bg: (u8, u8, u8),
    text_fg: (u8, u8, u8),
    text_secondary: (u8, u8, u8),
    primary: (u8, u8, u8),
    success: (u8, u8, u8),
    error: (u8, u8, u8),
    neutral: (u8, u8, u8),
    /// Whether this theme wants the dark title bar / dark control visual
    /// style, as opposed to the OS default (light) one.
    dark_chrome: bool,
}

const DARK_PALETTE: Palette = Palette {
    window_bg: (18, 18, 18),       // Material dark "background"
    control_bg: (32, 32, 35),      // Material dark "surface" (elevation 1)
    text_fg: (230, 230, 230),      // ~87% white, high-emphasis text
    text_secondary: (150, 150, 150), // ~60% white, medium-emphasis text
    primary: (138, 180, 248),      // Material blue 300 (dark-theme accent)
    success: (76, 175, 80),        // Material green 500
    error: (244, 67, 54),          // Material red 500
    neutral: (120, 120, 120),
    dark_chrome: true,
};

const LIGHT_PALETTE: Palette = Palette {
    window_bg: (245, 245, 245),
    control_bg: (255, 255, 255),
    text_fg: (33, 33, 33),
    text_secondary: (97, 97, 97),
    primary: (25, 118, 210),  // Material blue 700
    success: (56, 142, 60),   // Material green 700
    error: (211, 47, 47),     // Material red 700
    neutral: (117, 117, 117),
    dark_chrome: false,
};

const DRACULA_PALETTE: Palette = Palette {
    window_bg: (40, 42, 54),        // #282a36 "Background"
    control_bg: (68, 71, 90),       // #44475a "Current Line"/"Selection"
    text_fg: (248, 248, 242),       // #f8f8f2 "Foreground"
    text_secondary: (139, 150, 194), // lightened "Comment" for legibility
    primary: (189, 147, 249),       // #bd93f9 "Purple"
    success: (80, 250, 123),        // #50fa7b "Green"
    error: (255, 85, 85),           // #ff5555 "Red"
    neutral: (98, 114, 164),        // #6272a4 "Comment"
    dark_chrome: true,
};

fn palette_for(theme: Theme) -> Palette {
    match theme {
        Theme::Dark => DARK_PALETTE,
        Theme::Light => LIGHT_PALETTE,
        Theme::Dracula => DRACULA_PALETTE,
    }
}

thread_local! {
    static CURRENT_THEME: Cell<Theme> = Cell::new(Theme::Dark);
    static MAIN_WINDOWS: RefCell<Vec<usize>> = RefCell::new(Vec::new());
    static THEMED_CONTROLS: RefCell<Vec<usize>> = RefCell::new(Vec::new());
}

pub fn current_theme() -> Theme {
    CURRENT_THEME.with(|c| c.get())
}

fn palette() -> Palette {
    palette_for(current_theme())
}

// --- Palette accessors. Each call reads the *current* theme, so anything
// that paints by calling these (rather than caching the tuple once) repaints
// correctly after `set_theme`. ---
pub fn window_bg() -> (u8, u8, u8) {
    palette().window_bg
}
pub fn control_bg() -> (u8, u8, u8) {
    palette().control_bg
}
pub fn text_fg() -> (u8, u8, u8) {
    palette().text_fg
}
pub fn text_secondary() -> (u8, u8, u8) {
    palette().text_secondary
}
pub fn primary() -> (u8, u8, u8) {
    palette().primary
}
pub fn success() -> (u8, u8, u8) {
    palette().success
}
pub fn error() -> (u8, u8, u8) {
    palette().error
}
pub fn neutral() -> (u8, u8, u8) {
    palette().neutral
}

/// Which palette slot an owner-drawn control is colored from. Stored instead
/// of a raw RGB so theme switches repaint existing controls correctly.
#[derive(Clone, Copy)]
pub enum Role {
    Primary,
    Success,
    Error,
}

impl Role {
    fn rgb(self) -> (u8, u8, u8) {
        match self {
            Role::Primary => primary(),
            Role::Success => success(),
            Role::Error => error(),
        }
    }
}

#[derive(Clone, Copy)]
enum ButtonKind {
    /// Solid filled "contained" Material button - primary actions.
    Filled(Role),
    /// Flat, no fill at rest, colored label - secondary/text actions. Shows
    /// a soft tinted "state layer" on hover/press, like a Material text
    /// button's ripple/hover overlay.
    Text(Role),
}

fn rgb((r, g, b): (u8, u8, u8)) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Linearly interpolates from `base` towards `target` by `t` (0.0..=1.0).
/// Used to derive hover/pressed "state layer" shades from a button's base
/// color without needing true alpha blending.
fn blend(base: (u8, u8, u8), target: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let lerp = |b: u8, c: u8| -> u8 {
        (b as f32 + (c as f32 - b as f32) * t).round().clamp(0.0, 255.0) as u8
    };
    (lerp(base.0, target.0), lerp(base.1, target.1), lerp(base.2, target.2))
}

/// WM_DRAWITEM hands over a bare HDC with no font selected, so owner-drawn
/// text would otherwise fall back to the stock system font instead of
/// matching the rest of the (Segoe UI) dialog. The control's real font is
/// whatever it last got via WM_SETFONT - ask for it back and select it.
unsafe fn select_control_font(hdc: HDC, control_hwnd: HWND) {
    unsafe {
        let font = SendMessageW(control_hwnd, WM_GETFONT, 0, 0) as HFONT;
        if !font.is_null() {
            SelectObject(hdc, font as HGDIOBJ);
        }
    }
}

/// Measures how wide `text` renders in `hwnd`'s own font, in pixels. Used to
/// size a label tightly around its text instead of guessing a fixed width,
/// so a following control can be placed flush right after it.
pub fn measure_text_width(hwnd: HWND, text: &str) -> i32 {
    unsafe {
        let hdc = GetDC(hwnd);
        if hdc.is_null() {
            return 0;
        }
        let font = SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT;
        let old_font = if !font.is_null() { SelectObject(hdc, font as HGDIOBJ) } else { null_mut() };
        let wide = wide_null(text);
        let mut size: SIZE = std::mem::zeroed();
        GetTextExtentPoint32W(hdc, wide.as_ptr(), (wide.len() - 1) as i32, &mut size);
        if !old_font.is_null() {
            SelectObject(hdc, old_font);
        }
        ReleaseDC(hwnd, hdc);
        size.cx
    }
}

unsafe fn draw_centered_text(hdc: HDC, hwnd: HWND) {
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let mut text_buf = [0u16; 128];
        let len = GetWindowTextW(hwnd, text_buf.as_mut_ptr(), text_buf.len() as i32);
        DrawTextW(hdc, text_buf.as_ptr(), len, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }
}

thread_local! {
    static WINDOW_BRUSH: Cell<HBRUSH> = Cell::new(null_mut());
    static CONTROL_BRUSH: Cell<HBRUSH> = Cell::new(null_mut());
    static BRUSH_CACHE: RefCell<HashMap<(u8, u8, u8), HBRUSH>> = RefCell::new(HashMap::new());
    static BUTTON_STYLES: RefCell<HashMap<usize, ButtonKind>> = RefCell::new(HashMap::new());
    static OWNER_DRAW_TOGGLES: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    // On/off state for owner-drawn toggle switches, tracked ourselves rather
    // than through BM_SETCHECK/BM_GETCHECK: those only work on checkbox-family
    // button styles, and `to_owner_draw` replaces the style's type nibble with
    // plain BS_OWNERDRAW - so BM_SETCHECK silently doesn't stick.
    static CONTROL_STATE: RefCell<HashMap<usize, bool>> = RefCell::new(HashMap::new());
    static STATIC_TEXT_COLORS: RefCell<HashMap<usize, (u8, u8, u8)>> = RefCell::new(HashMap::new());
    static HOVERED: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    // (owning window, rect, color). Scoped per window because every window
    // with `install_dark_background` shares the same erase handler.
    static ACCENT_BARS: RefCell<Vec<(usize, RECT, Role)>> = RefCell::new(Vec::new());
}

/// Sets an owner-drawn toggle's on/off state and repaints it.
/// See `CONTROL_STATE` for why this exists.
pub fn set_control_state(hwnd: HWND, on: bool) {
    CONTROL_STATE.with(|m| m.borrow_mut().insert(hwnd as usize, on));
    unsafe {
        InvalidateRect(hwnd, null_mut(), 1);
    }
}

/// Reads back the on/off state last set with `set_control_state` (defaults
/// to `false` for a control that's never had one set).
pub fn control_state(hwnd: HWND) -> bool {
    CONTROL_STATE.with(|m| m.borrow().get(&(hwnd as usize)).copied().unwrap_or(false))
}

/// Registers a thin solid-color accent bar (used as the Material "filled
/// text field" underline) painted directly by the window's background
/// erase, rather than as a real child control - a real `Label` forces a
/// minimum height of one text line (even with empty text) via its own
/// vertical-centering `WM_NCCALCSIZE` hook, which fights a bar only a few
/// pixels tall. Only painted in `owner`. Returns an id to reposition it
/// later with `set_accent_bar_rect`.
pub fn register_accent_bar(owner: HWND, role: Role) -> usize {
    ACCENT_BARS.with(|bars| {
        let mut bars = bars.borrow_mut();
        bars.push((owner as usize, RECT { left: 0, top: 0, right: 0, bottom: 0 }, role));
        bars.len() - 1
    })
}

/// Moves/resizes a previously registered accent bar. Takes effect on the
/// next background erase (e.g. after a resize).
pub fn set_accent_bar_rect(id: usize, x: i32, y: i32, w: i32, h: i32) {
    ACCENT_BARS.with(|bars| {
        if let Some(entry) = bars.borrow_mut().get_mut(id) {
            entry.1 = RECT { left: x, top: y, right: x + w, bottom: y + h };
        }
    });
}

/// Overrides the text color a specific static label (WM_CTLCOLORSTATIC)
/// draws with, instead of the default `text_fg()`. Used for the status
/// readouts, which need to turn green/red/gray depending on state.
pub fn set_static_text_color(hwnd: HWND, color: (u8, u8, u8)) {
    STATIC_TEXT_COLORS.with(|m| m.borrow_mut().insert(hwnd as usize, color));
    unsafe {
        InvalidateRect(hwnd, null_mut(), 1);
    }
}

fn cached_brush(color: (u8, u8, u8)) -> HBRUSH {
    BRUSH_CACHE.with(|m| {
        let mut map = m.borrow_mut();
        *map.entry(color).or_insert_with(|| unsafe { CreateSolidBrush(rgb(color)) })
    })
}

/// Fully disables visual-style theming on a control (empty sub-app/id-list
/// is the standard trick for this). Owner-drawn controls should always do
/// this first - otherwise Windows can still paint a themed frame or focus
/// decoration on top of/under whatever WM_DRAWITEM draws, which is exactly
/// the stray border seen around the owner-drawn buttons before this.
fn disable_theming(hwnd: HWND) {
    unsafe {
        let blank = wide_null(" ");
        SetWindowTheme(hwnd, blank.as_ptr(), blank.as_ptr());
    }
}

fn window_brush() -> HBRUSH {
    WINDOW_BRUSH.with(|cell| {
        let mut b = cell.get();
        if b.is_null() {
            b = unsafe { CreateSolidBrush(rgb(window_bg())) };
            cell.set(b);
        }
        b
    })
}

fn control_brush() -> HBRUSH {
    CONTROL_BRUSH.with(|cell| {
        let mut b = cell.get();
        if b.is_null() {
            b = unsafe { CreateSolidBrush(rgb(control_bg())) };
            cell.set(b);
        }
        b
    })
}

/// Opts the whole process into dark-capable common controls. Call once,
/// before creating any windows.
pub fn enable_app_dark_mode() {
    unsafe {
        let lib_name = CString::new("uxtheme.dll").unwrap();
        let lib = LoadLibraryA(lib_name.as_ptr());
        if lib.is_null() {
            return;
        }
        // Ordinal 135 = SetPreferredAppMode(AllowDark = 1)
        let proc = GetProcAddress(lib, 135usize as LPCSTR);
        if !proc.is_null() {
            let set_preferred_app_mode: unsafe extern "system" fn(c_int) -> c_int =
                transmute(proc);
            set_preferred_app_mode(1);
        }
    }
}

/// Dark- or light-themes a top-level window's title bar/border to match the
/// current theme. Safe to call again after a theme switch - `set_theme`
/// does this for every window registered via `install_dark_background`.
pub fn apply_titlebar_theme(hwnd: HWND) {
    unsafe {
        let want_dark = palette().dark_chrome;
        let value: BOOL = if want_dark { TRUE } else { 0 };
        let ok = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const _ as *const _,
            std::mem::size_of::<BOOL>() as DWORD,
        );
        if ok != 0 {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE_OLD,
                &value as *const _ as *const _,
                std::mem::size_of::<BOOL>() as DWORD,
            );
        }
        // Nudge the non-client area to redraw with the new attribute
        // immediately, instead of waiting for the next user-driven resize.
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

unsafe fn set_dark_mode_for_window(hwnd: HWND, allow: bool) {
    unsafe {
        let lib_name = CString::new("uxtheme.dll").unwrap();
        let lib = LoadLibraryA(lib_name.as_ptr());
        if !lib.is_null() {
            // Ordinal 133 = AllowDarkModeForWindow(hwnd, allow)
            let proc = GetProcAddress(lib, 133usize as LPCSTR);
            if !proc.is_null() {
                let allow_dark_mode_for_window: unsafe extern "system" fn(HWND, BOOL) -> BOOL =
                    transmute(proc);
                allow_dark_mode_for_window(hwnd, allow as BOOL);
            }
        }
        if allow {
            let theme = wide_null("DarkMode_Explorer");
            SetWindowTheme(hwnd, theme.as_ptr(), null_mut());
        } else {
            SetWindowTheme(hwnd, null_mut(), null_mut());
        }
    }
}

/// Switches a non-owner-drawn control (e.g. a text field) to the current
/// theme's visual-style variant, and remembers it so `set_theme` can re-apply
/// the right variant later if the theme changes while the app is running.
pub fn apply_dark_control(hwnd: HWND) {
    THEMED_CONTROLS.with(|v| v.borrow_mut().push(hwnd as usize));
    unsafe { set_dark_mode_for_window(hwnd, palette().dark_chrome) };
}

/// Subclasses a single owner-drawn control so we notice WM_MOUSEMOVE/
/// WM_MOUSELEAVE on it directly (mouse messages go to the control under the
/// cursor, not its parent) and can repaint it with a hover "state layer".
const HOVER_SUBCLASS_ID: usize = 0x5647_5F48; // 'VG_H'

unsafe extern "system" fn button_hover_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEMOVE => {
                let already_hovered = HOVERED.with(|m| m.borrow().contains(&(hwnd as usize)));
                if !already_hovered {
                    HOVERED.with(|m| m.borrow_mut().insert(hwnd as usize));
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut tme);
                    InvalidateRect(hwnd, null_mut(), 0);
                }
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            WM_MOUSELEAVE => {
                HOVERED.with(|m| m.borrow_mut().remove(&(hwnd as usize)));
                InvalidateRect(hwnd, null_mut(), 0);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
}

fn install_hover_tracking(hwnd: HWND) {
    unsafe {
        SetWindowSubclass(hwnd, Some(button_hover_proc), HOVER_SUBCLASS_ID, 0);
    }
}

fn to_owner_draw(hwnd: HWND) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !BS_TYPEMASK) | BS_OWNERDRAW as isize);
    }
    disable_theming(hwnd);
}

/// Styles a button as a Material "contained" (filled) button: a solid
/// rounded-rect fill in `role`'s current-theme color with white text, used
/// for primary actions. Lightens on hover, darkens when pressed.
pub fn make_filled_button(hwnd: HWND, role: Role) {
    to_owner_draw(hwnd);
    install_hover_tracking(hwnd);
    BUTTON_STYLES.with(|m| m.borrow_mut().insert(hwnd as usize, ButtonKind::Filled(role)));
}

/// Same idea as `make_filled_button`, but flat/text-styled for secondary
/// actions - see `ButtonKind::Text`.
pub fn make_text_button(hwnd: HWND, role: Role) {
    to_owner_draw(hwnd);
    install_hover_tracking(hwnd);
    BUTTON_STYLES.with(|m| m.borrow_mut().insert(hwnd as usize, ButtonKind::Text(role)));
}

/// Styles a plain `Button` as an iOS-style on/off toggle switch (a pill
/// track with a sliding knob) instead of drawing its own text - pair it with
/// a separate `Label` for the caption. Its state is driven through
/// `set_control_state`/`control_state`.
pub fn make_dark_toggle(hwnd: HWND) {
    to_owner_draw(hwnd);
    OWNER_DRAW_TOGGLES.with(|m| m.borrow_mut().insert(hwnd as usize));
}

unsafe fn round_rect_fill(hdc: HDC, rect: RECT, radius: i32, color: (u8, u8, u8)) {
    unsafe {
        let brush = CreateSolidBrush(rgb(color));
        let pen = CreatePen(PS_SOLID as i32, 1, rgb(color));
        let old_brush = SelectObject(hdc, brush as HGDIOBJ);
        let old_pen = SelectObject(hdc, pen as HGDIOBJ);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush as HGDIOBJ);
        DeleteObject(pen as HGDIOBJ);
    }
}

unsafe fn draw_button(dis: &DRAWITEMSTRUCT, kind: ButtonKind, hovered: bool) {
    unsafe {
        let hdc = dis.hDC;
        let rect = dis.rcItem;
        let pressed = dis.itemState & ODS_SELECTED != 0;

        let text_color = match kind {
            ButtonKind::Filled(role) => {
                let color = role.rgb();
                let fill = if pressed {
                    blend(color, (0, 0, 0), 0.2)
                } else if hovered {
                    blend(color, (255, 255, 255), 0.12)
                } else {
                    color
                };
                round_rect_fill(hdc, rect, BUTTON_RADIUS, fill);
                (255, 255, 255)
            }
            ButtonKind::Text(role) => {
                let accent = role.rgb();
                // A Material "tonal" button: always shows a muted tint of
                // its accent color (so it reads as a button at rest, not
                // just colored text), growing stronger on hover/press.
                let tint_t = if pressed {
                    0.32
                } else if hovered {
                    0.22
                } else {
                    0.12
                };
                round_rect_fill(hdc, rect, BUTTON_RADIUS, blend(window_bg(), accent, tint_t));
                accent
            }
        };

        select_control_font(hdc, dis.hwndItem);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, rgb(text_color));
        draw_centered_text(hdc, dis.hwndItem);
    }
}

/// Draws an iOS-style on/off switch: a pill-shaped track (tinted `primary()`
/// when on, a muted neutral tone when off) with a white knob slid to
/// whichever side matches the state `make_dark_toggle` controls track.
unsafe fn draw_toggle(dis: &DRAWITEMSTRUCT) {
    unsafe {
        let hdc = dis.hDC;
        let rect = dis.rcItem;

        let bg_brush = CreateSolidBrush(rgb(window_bg()));
        FillRect(hdc, &rect, bg_brush);
        DeleteObject(bg_brush as HGDIOBJ);

        let disabled = dis.itemState & ODS_DISABLED != 0;
        let on = control_state(dis.hwndItem);

        let track_h = (rect.bottom - rect.top).min(22);
        let track_w = (rect.right - rect.left).min(track_h * 2 + 10).max(track_h * 2);
        let track_top = rect.top + (rect.bottom - rect.top - track_h) / 2;
        let track_left = rect.left;
        let track_rect = RECT {
            left: track_left,
            top: track_top,
            right: track_left + track_w,
            bottom: track_top + track_h,
        };

        let track_color = if disabled {
            blend(window_bg(), neutral(), 0.4)
        } else if on {
            primary()
        } else {
            blend(window_bg(), text_secondary(), 0.6)
        };
        round_rect_fill(hdc, track_rect, track_h / 2, track_color);

        let knob_d = track_h - 4;
        let knob_top = track_top + 2;
        let knob_left = if on { track_rect.right - knob_d - 2 } else { track_rect.left + 2 };
        let knob_rect =
            RECT { left: knob_left, top: knob_top, right: knob_left + knob_d, bottom: knob_top + knob_d };
        round_rect_fill(hdc, knob_rect, knob_d / 2, (255, 255, 255));
    }
}

unsafe extern "system" fn dark_background_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let hdc = wparam as HDC;
                SetTextColor(hdc, rgb(text_fg()));
                SetBkColor(hdc, rgb(control_bg()));
                control_brush() as LRESULT
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let hdc = wparam as HDC;
                let ctrl_hwnd = lparam as HWND;
                let text_color = STATIC_TEXT_COLORS
                    .with(|m| m.borrow().get(&(ctrl_hwnd as usize)).copied())
                    .unwrap_or_else(text_fg);
                SetTextColor(hdc, rgb(text_color));
                SetBkColor(hdc, rgb(window_bg()));
                // OPAQUE (not TRANSPARENT): with ClearType font smoothing, a
                // transparent background blends each glyph's anti-aliased
                // edges against whatever pixels are already on the surface,
                // rather than the color just set above. If that surface
                // still held stale content from a previous frame (e.g. the
                // control's own background erase hadn't landed yet), the
                // blend uses the wrong color and part of the text renders
                // near-invisible against the dark background. OPAQUE makes
                // every glyph redraw its own cell background from
                // `window_bg()` right before stroking it, so stale pixels
                // underneath can't leak through - this matches the solid
                // brush already used to erase the whole control, so it's a
                // no-op visually in the normal case.
                SetBkMode(hdc, OPAQUE as i32);
                window_brush() as LRESULT
            }
            WM_DRAWITEM => {
                let dis = &*(lparam as *const DRAWITEMSTRUCT);
                let item_hwnd = dis.hwndItem as usize;
                let style = BUTTON_STYLES.with(|m| m.borrow().get(&item_hwnd).copied());
                let is_toggle = OWNER_DRAW_TOGGLES.with(|m| m.borrow().contains(&item_hwnd));
                if let Some(kind) = style {
                    let hovered = HOVERED.with(|m| m.borrow().contains(&item_hwnd));
                    draw_button(dis, kind, hovered);
                    1
                } else if is_toggle {
                    draw_toggle(dis);
                    1
                } else {
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
            }
            WM_ERASEBKGND => {
                let hdc = wparam as HDC;
                let mut rect = std::mem::zeroed();
                GetClientRect(hwnd, &mut rect);
                FillRect(hdc, &rect, window_brush());
                ACCENT_BARS.with(|bars| {
                    for (owner, bar_rect, role) in bars.borrow().iter() {
                        if *owner == hwnd as usize {
                            FillRect(hdc, bar_rect, cached_brush(role.rgb()));
                        }
                    }
                });
                1
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
}

/// Installs the WM_CTLCOLOR*/WM_ERASEBKGND dark-background handling on a
/// top-level window. The window's Edit/Static children get their colors
/// from here since neither has a dark visual-style variant. Also remembers
/// the window so `set_theme` can re-theme its title bar and force a full
/// repaint later.
// Distinctive id so we don't collide with nwg's own internal subclass ids.
const SUBCLASS_ID: usize = 0x5647_5F44; // 'VG_D'

pub fn install_dark_background(hwnd: HWND) {
    MAIN_WINDOWS.with(|v| v.borrow_mut().push(hwnd as usize));
    unsafe {
        SetWindowSubclass(hwnd, Some(dark_background_proc), SUBCLASS_ID, 0);
    }
}

/// Switches the active theme and repaints everything that's already on
/// screen: cached background brushes are rebuilt, every window registered
/// via `install_dark_background` gets its title bar re-themed and a forced
/// full repaint, and every control registered via `apply_dark_control` gets
/// its visual-style variant re-applied. Also fine to call before any window
/// exists (e.g. at startup, to apply the theme loaded from `Config`) - the
/// window/control loops are simply no-ops until something registers.
pub fn set_theme(theme: Theme) {
    CURRENT_THEME.with(|c| c.set(theme));

    for cell in [&WINDOW_BRUSH, &CONTROL_BRUSH] {
        cell.with(|c| {
            let b = c.get();
            if !b.is_null() {
                unsafe {
                    DeleteObject(b as HGDIOBJ);
                }
                c.set(null_mut());
            }
        });
    }
    BRUSH_CACHE.with(|m| {
        for (_, b) in m.borrow_mut().drain() {
            unsafe {
                DeleteObject(b as HGDIOBJ);
            }
        }
    });

    THEMED_CONTROLS.with(|v| {
        for &hwnd in v.borrow().iter() {
            unsafe { set_dark_mode_for_window(hwnd as HWND, palette().dark_chrome) };
        }
    });

    MAIN_WINDOWS.with(|v| {
        for &hwnd in v.borrow().iter() {
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
    });
}
