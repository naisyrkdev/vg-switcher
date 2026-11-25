//! Owner-drawn controls: Material-style filled/tonal buttons, iOS-style
//! toggle switches, accent bars under text fields, and per-label text
//! colours. All state is keyed by HWND and dropped when the control is
//! destroyed (see `win32::control_proc`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::mem::size_of;
use std::ptr::null_mut;

use winapi::shared::windef::{HDC, HFONT, HWND, RECT, SIZE};
use winapi::um::wingdi::{
    CreatePen, DeleteObject, GetTextExtentPoint32W, PS_SOLID, RoundRect, SelectObject, SetBkMode, SetTextColor,
    TRANSPARENT,
};
use winapi::um::winuser::{
    DRAWITEMSTRUCT, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DrawTextW, FillRect, GWL_STYLE, GetDC, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, InvalidateRect, ODS_DISABLED, ODS_SELECTED, ReleaseDC, SendMessageW,
    SetWindowLongPtrW, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, WM_GETFONT,
};

use super::win32::{brush, install_control_subclass};
use super::{BLACK, Rgb, Role, WHITE, blend, colorref, neutral, primary, text_secondary, window_bg};
use crate::platform::wide;

const BS_OWNERDRAW: isize = 0x0000_000B;
const BS_TYPEMASK: isize = 0x0000_000F;

/// Corner radius of buttons.
const BUTTON_RADIUS: i32 = 10;

#[derive(Debug, Clone, Copy)]
enum Kind {
    /// Solid "contained" button - primary actions.
    Filled(Role),
    /// Tonal button: a muted tint of its colour at rest, stronger on
    /// hover/press - secondary actions.
    Text(Role),
    /// On/off switch. The state is tracked here, not with BM_SETCHECK,
    /// because an owner-drawn button style doesn't keep a check state.
    Toggle { on: bool },
}

#[derive(Debug, Clone, Copy)]
struct OwnerDrawn {
    kind: Kind,
    hovered: bool,
}

struct AccentBarEntry {
    owner: usize,
    rect: RECT,
    role: Role,
}

/// Handle to a bar registered with [`register_accent_bar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccentBar(u32);

thread_local! {
    static OWNER_DRAWN: RefCell<HashMap<usize, OwnerDrawn>> = RefCell::new(HashMap::new());
    static STATIC_TEXT_COLORS: RefCell<HashMap<usize, Rgb>> = RefCell::new(HashMap::new());
    static ACCENT_BARS: RefCell<HashMap<AccentBar, AccentBarEntry>> = RefCell::new(HashMap::new());
    static NEXT_BAR_ID: Cell<u32> = const { Cell::new(0) };
}

fn register(hwnd: HWND, kind: Kind) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !BS_TYPEMASK) | BS_OWNERDRAW);
        // Disable visual styles entirely (an empty theme name is the
        // standard trick) so Windows paints no themed frame or focus
        // decoration around what WM_DRAWITEM draws.
        let blank = wide(" ");
        winapi::um::uxtheme::SetWindowTheme(hwnd, blank.as_ptr(), blank.as_ptr());
    }
    OWNER_DRAWN.with(|m| m.borrow_mut().insert(hwnd as usize, OwnerDrawn { kind, hovered: false }));
    install_control_subclass(hwnd);
}

/// Styles a button as a filled button in `role`'s colour with white text.
pub fn make_filled_button(hwnd: HWND, role: Role) {
    register(hwnd, Kind::Filled(role));
}

/// Styles a button as a tonal (secondary) button in `role`'s colour.
pub fn make_text_button(hwnd: HWND, role: Role) {
    register(hwnd, Kind::Text(role));
}

/// Styles a button as an on/off switch with no text of its own - pair it
/// with a label. Driven by [`set_toggle_state`] / [`toggle_state`].
pub fn make_toggle(hwnd: HWND) {
    register(hwnd, Kind::Toggle { on: false });
}

pub fn set_toggle_state(hwnd: HWND, on: bool) {
    OWNER_DRAWN.with(|m| {
        if let Some(OwnerDrawn { kind: Kind::Toggle { on: state }, .. }) = m.borrow_mut().get_mut(&(hwnd as usize)) {
            *state = on;
        }
    });
    unsafe {
        InvalidateRect(hwnd, null_mut(), 1);
    }
}

pub fn toggle_state(hwnd: HWND) -> bool {
    OWNER_DRAWN
        .with(|m| matches!(m.borrow().get(&(hwnd as usize)), Some(OwnerDrawn { kind: Kind::Toggle { on: true }, .. })))
}

/// Overrides the text colour of one static label (default: `text_fg()`).
pub fn set_static_text_color(hwnd: HWND, color: Rgb) {
    let first = STATIC_TEXT_COLORS.with(|m| m.borrow_mut().insert(hwnd as usize, color).is_none());
    if first {
        install_control_subclass(hwnd);
    }
    unsafe {
        InvalidateRect(hwnd, null_mut(), 1);
    }
}

pub(super) fn static_text_color(hwnd: HWND) -> Option<Rgb> {
    STATIC_TEXT_COLORS.with(|m| m.borrow().get(&(hwnd as usize)).copied())
}

/// Registers a thin bar (a Material text-field underline) painted by
/// `owner`'s background erase. A real Label can't be this short: nwg's
/// Label forces at least one text line of height.
pub fn register_accent_bar(owner: HWND, role: Role) -> AccentBar {
    let id = AccentBar(NEXT_BAR_ID.with(|n| n.replace(n.get() + 1)));
    let entry = AccentBarEntry { owner: owner as usize, rect: RECT { left: 0, top: 0, right: 0, bottom: 0 }, role };
    ACCENT_BARS.with(|m| m.borrow_mut().insert(id, entry));
    id
}

/// Moves a bar. Takes effect on the owner's next background erase.
pub fn set_accent_bar_rect(bar: AccentBar, x: i32, y: i32, w: i32, h: i32) {
    ACCENT_BARS.with(|m| {
        if let Some(entry) = m.borrow_mut().get_mut(&bar) {
            entry.rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
        }
    });
}

pub(super) fn paint_accent_bars(hdc: HDC, owner: HWND) {
    ACCENT_BARS.with(|m| {
        for entry in m.borrow().values().filter(|e| e.owner == owner as usize) {
            unsafe {
                FillRect(hdc, &entry.rect, brush(entry.role.rgb()));
            }
        }
    });
}

pub(super) fn forget(hwnd: HWND) {
    OWNER_DRAWN.with(|m| m.borrow_mut().remove(&(hwnd as usize)));
    STATIC_TEXT_COLORS.with(|m| m.borrow_mut().remove(&(hwnd as usize)));
}

pub(super) fn forget_owner(owner: HWND) {
    ACCENT_BARS.with(|m| m.borrow_mut().retain(|_, e| e.owner != owner as usize));
}

fn set_hovered(hwnd: HWND, hovered: bool) -> bool {
    OWNER_DRAWN.with(|m| match m.borrow_mut().get_mut(&(hwnd as usize)) {
        Some(c) if matches!(c.kind, Kind::Filled(_) | Kind::Text(_)) && c.hovered != hovered => {
            c.hovered = hovered;
            true
        }
        _ => false,
    })
}

pub(super) fn on_mouse_move(hwnd: HWND) {
    if set_hovered(hwnd, true) {
        unsafe {
            #[allow(clippy::cast_possible_truncation)]
            let mut tme = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tme);
            InvalidateRect(hwnd, null_mut(), 0);
        }
    }
}

pub(super) fn on_mouse_leave(hwnd: HWND) {
    if set_hovered(hwnd, false) {
        unsafe {
            InvalidateRect(hwnd, null_mut(), 0);
        }
    }
}

/// Paints an owner-drawn control. `false` if it isn't one of ours.
pub(super) fn draw_item(dis: &DRAWITEMSTRUCT) -> bool {
    let Some(control) = OWNER_DRAWN.with(|m| m.borrow().get(&(dis.hwndItem as usize)).copied()) else {
        return false;
    };
    unsafe {
        match control.kind {
            Kind::Filled(_) | Kind::Text(_) => draw_button(dis, control.kind, control.hovered),
            Kind::Toggle { on } => draw_toggle(dis, on),
        }
    }
    true
}

/// Width in pixels of `text` in `hwnd`'s font, to size a label to its text.
pub fn measure_text_width(hwnd: HWND, text: &str) -> i32 {
    unsafe {
        let hdc = GetDC(hwnd);
        if hdc.is_null() {
            return 0;
        }
        let old_font = select_control_font(hdc, hwnd);
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut size: SIZE = std::mem::zeroed();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        GetTextExtentPoint32W(hdc, units.as_ptr(), units.len() as i32, &mut size);
        restore_font(hdc, old_font);
        ReleaseDC(hwnd, hdc);
        size.cx
    }
}

/// Selects the control's own font (owner-draw DCs come with the system
/// font). Returns the previous font to hand to `restore_font`.
unsafe fn select_control_font(hdc: HDC, hwnd: HWND) -> HFONT {
    unsafe {
        let font = SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT;
        if font.is_null() { null_mut() } else { SelectObject(hdc, font.cast()).cast() }
    }
}

unsafe fn restore_font(hdc: HDC, old: HFONT) {
    if !old.is_null() {
        unsafe {
            SelectObject(hdc, old.cast());
        }
    }
}

unsafe fn round_rect_fill(hdc: HDC, rect: RECT, radius: i32, color: Rgb) {
    unsafe {
        let pen = CreatePen(PS_SOLID as i32, 1, colorref(color));
        let old_brush = SelectObject(hdc, brush(color).cast());
        let old_pen = SelectObject(hdc, pen.cast());
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(pen.cast());
    }
}

unsafe fn draw_button(dis: &DRAWITEMSTRUCT, kind: Kind, hovered: bool) {
    let pressed = dis.itemState & ODS_SELECTED != 0;
    let (fill, text_color) = match kind {
        Kind::Filled(role) => {
            let color = role.rgb();
            let fill = if pressed {
                blend(color, BLACK, 0.2)
            } else if hovered {
                blend(color, WHITE, 0.12)
            } else {
                color
            };
            (fill, WHITE)
        }
        Kind::Text(role) => {
            let tint = if pressed {
                0.32
            } else if hovered {
                0.22
            } else {
                0.12
            };
            (blend(window_bg(), role.rgb(), tint), role.rgb())
        }
        Kind::Toggle { .. } => unreachable!("toggles are drawn by draw_toggle"),
    };
    unsafe {
        let hdc = dis.hDC;
        // Clear the corners outside the rounded rect.
        FillRect(hdc, &dis.rcItem, brush(window_bg()));
        round_rect_fill(hdc, dis.rcItem, BUTTON_RADIUS, fill);

        let old_font = select_control_font(hdc, dis.hwndItem);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, colorref(text_color));
        #[allow(clippy::cast_sign_loss)]
        let len = GetWindowTextLengthW(dis.hwndItem).max(0) as usize;
        let mut text = vec![0u16; len + 1];
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let copied = GetWindowTextW(dis.hwndItem, text.as_mut_ptr(), text.len() as i32);
        let mut rect = dis.rcItem;
        DrawTextW(hdc, text.as_ptr(), copied, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        restore_font(hdc, old_font);
    }
}

/// A pill-shaped track (primary colour when on, muted when off) with a
/// white knob on the matching side.
unsafe fn draw_toggle(dis: &DRAWITEMSTRUCT, on: bool) {
    let rect = dis.rcItem;
    let disabled = dis.itemState & ODS_DISABLED != 0;

    let track_h = (rect.bottom - rect.top).min(22);
    let track_w = (rect.right - rect.left).min(track_h * 2 + 10).max(track_h * 2);
    let track_top = rect.top + (rect.bottom - rect.top - track_h) / 2;
    let track = RECT { left: rect.left, top: track_top, right: rect.left + track_w, bottom: track_top + track_h };

    let track_color = if disabled {
        blend(window_bg(), neutral(), 0.4)
    } else if on {
        primary()
    } else {
        blend(window_bg(), text_secondary(), 0.6)
    };

    let knob_d = track_h - 4;
    let knob_left = if on { track.right - knob_d - 2 } else { track.left + 2 };
    let knob = RECT { left: knob_left, top: track_top + 2, right: knob_left + knob_d, bottom: track_top + 2 + knob_d };

    unsafe {
        FillRect(dis.hDC, &rect, brush(window_bg()));
        round_rect_fill(dis.hDC, track, track_h / 2, track_color);
        round_rect_fill(dis.hDC, knob, knob_d / 2, WHITE);
    }
}
