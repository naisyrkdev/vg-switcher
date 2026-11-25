//! Reusable, already-themed controls and layout helpers. New windows
//! should be built from these so they follow the AGENTS.md UI rules
//! without having to repeat them.

use native_windows_gui as nwg;
use winapi::shared::windef::HWND;

use super::{H_GAP, LABEL_H, MARGIN, STATUS_H, TOGGLE_H, TOGGLE_W};
use crate::error::Result;
use crate::theme::{self, Rgb, Role};

/// The HWND of a control that has been built.
pub fn hwnd(handle: &nwg::ControlHandle) -> HWND {
    handle.hwnd().expect("control was built, so it has an HWND")
}

/// Something that can be given a position and size.
pub trait Placeable {
    fn place(&self, x: i32, y: i32, w: i32, h: i32);
}

macro_rules! impl_placeable {
    ($($t:ty),*) => {$(
        impl Placeable for $t {
            #[allow(clippy::cast_sign_loss)]
            fn place(&self, x: i32, y: i32, w: i32, h: i32) {
                self.set_position(x, y);
                self.set_size(w.max(0) as u32, h.max(0) as u32);
            }
        }
    )*};
}
impl_placeable!(nwg::Button, nwg::Label, nwg::TextBox, nwg::TextInput);

/// Places `items` (control, width) left to right from `x`, `H_GAP` apart.
pub fn place_row(x: i32, y: i32, h: i32, items: &[(&dyn Placeable, i32)]) {
    let mut x = x;
    for (control, w) in items {
        control.place(x, y, *w, h);
        x += w + H_GAP;
    }
}

/// Like [`place_row`], but the row ends at `right` (never starting left of
/// `MARGIN`).
pub fn place_row_right(right: i32, y: i32, h: i32, items: &[(&dyn Placeable, i32)]) {
    let total: i32 = items.iter().map(|(_, w)| w).sum::<i32>() + H_GAP * (items.len() as i32 - 1).max(0);
    place_row((right - total).max(MARGIN), y, h, items);
}

/// Creates a hidden, themed top-level window. Show it with `ui::present`
/// once its controls are built and laid out, so it never appears half-built.
pub fn themed_window(
    title: &str,
    client_size: (i32, i32),
    position: (i32, i32),
    resizable: bool,
    icon: &nwg::Icon,
) -> Result<nwg::Window> {
    let flags = if resizable { nwg::WindowFlags::MAIN_WINDOW } else { nwg::WindowFlags::WINDOW };
    let mut window = nwg::Window::default();
    nwg::Window::builder()
        .flags(flags)
        .size(client_size)
        .position(position)
        .title(title)
        .icon(Some(icon))
        .build(&mut window)?;
    let h = hwnd(&window.handle);
    theme::apply_titlebar_theme(h);
    theme::install_themed_background(h);
    Ok(window)
}

/// A one-line label. Keep it `LABEL_H` tall (see AGENTS.md).
pub fn label(parent: &nwg::Window, text: &str, font: Option<&nwg::Font>) -> Result<nwg::Label> {
    let mut label = nwg::Label::default();
    nwg::Label::builder().text(text).parent(parent).font(font).build(&mut label)?;
    Ok(label)
}

fn raw_button(parent: &nwg::Window, text: &str) -> Result<nwg::Button> {
    let mut button = nwg::Button::default();
    nwg::Button::builder().text(text).parent(parent).build(&mut button)?;
    Ok(button)
}

/// A filled (primary action) button.
pub fn filled_button(parent: &nwg::Window, text: &str, role: Role) -> Result<nwg::Button> {
    let button = raw_button(parent, text)?;
    theme::make_filled_button(hwnd(&button.handle), role);
    Ok(button)
}

/// A tonal (secondary action) button.
pub fn text_button(parent: &nwg::Window, text: &str, role: Role) -> Result<nwg::Button> {
    let button = raw_button(parent, text)?;
    theme::make_text_button(hwnd(&button.handle), role);
    Ok(button)
}

/// An on/off switch with its caption to the right.
pub struct Toggle {
    pub switch: nwg::Button,
    caption: nwg::Label,
}

impl Toggle {
    pub fn build(parent: &nwg::Window, caption: &str) -> Result<Self> {
        let switch = raw_button(parent, "")?;
        theme::make_toggle(hwnd(&switch.handle));
        Ok(Toggle { switch, caption: label(parent, caption, None)? })
    }

    pub fn is_on(&self) -> bool {
        theme::toggle_state(hwnd(&self.switch.handle))
    }

    pub fn set_on(&self, on: bool) {
        theme::set_toggle_state(hwnd(&self.switch.handle), on);
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.switch.set_enabled(enabled);
    }

    /// One row: switch at the left, caption vertically centred next to it.
    pub fn place(&self, y: i32, content_w: i32) {
        self.switch.place(MARGIN, y, TOGGLE_W, TOGGLE_H);
        let caption_x = MARGIN + TOGGLE_W + H_GAP;
        self.caption.place(
            caption_x,
            y + (TOGGLE_H - LABEL_H) / 2,
            (content_w - (caption_x - MARGIN)).max(60),
            LABEL_H,
        );
    }
}

/// "Prefix:" followed by a coloured value, right after the prefix's
/// rendered text width.
pub struct StatusRow {
    prefix: nwg::Label,
    prefix_text: &'static str,
    value: nwg::Label,
}

impl StatusRow {
    pub fn build(parent: &nwg::Window, prefix_text: &'static str, value_font: &nwg::Font) -> Result<Self> {
        Ok(StatusRow {
            prefix: label(parent, prefix_text, None)?,
            prefix_text,
            value: label(parent, "", Some(value_font))?,
        })
    }

    pub fn set(&self, text: &str, color: Rgb) {
        // Colour first, so the repaint triggered by the text change already
        // uses it.
        theme::set_static_text_color(hwnd(&self.value.handle), color);
        self.value.set_text(text);
    }

    pub fn place(&self, y: i32, content_w: i32) {
        let y = y + (STATUS_H - LABEL_H) / 2;
        let prefix_w = theme::measure_text_width(hwnd(&self.prefix.handle), self.prefix_text);
        self.prefix.place(MARGIN, y, prefix_w + 4, LABEL_H);
        let value_x = MARGIN + prefix_w + H_GAP;
        self.value.place(value_x, y, (content_w - (value_x - MARGIN)).max(60), LABEL_H);
    }
}
