//! App themes (Dark / Light / Dracula) and how classic Win32 controls get
//! painted in them.
//!
//! - this file: the theme model - `Theme`, palettes, `Role`, colour accessors.
//! - `win32`: title bars, the undocumented uxtheme dark-mode switches,
//!   the background subclass that answers `WM_CTLCOLOR*` / `WM_ERASEBKGND`,
//!   and repainting everything on a theme switch.
//! - `controls`: owner-drawn buttons and toggle switches, accent bars,
//!   per-label text colours.
//!
//! Every colour is read through a function (`window_bg()`, `primary()`...)
//! that looks at the *current* theme, and owner-drawn controls store a
//! `Role` rather than an RGB value, so switching themes repaints everything
//! without recreating controls.

mod controls;
mod win32;

use std::cell::Cell;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use controls::{
    AccentBar, make_filled_button, make_text_button, make_toggle, measure_text_width, register_accent_bar,
    set_accent_bar_rect, set_static_text_color, set_toggle_state, toggle_state,
};
pub use win32::{apply_themed_control, apply_titlebar_theme, enable_app_dark_mode, install_themed_background};

pub type Rgb = (u8, u8, u8);

pub const WHITE: Rgb = (255, 255, 255);
const BLACK: Rgb = (0, 0, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Dracula,
}

impl Theme {
    /// All themes, in the order the General Settings window lists them.
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dracula, Theme::Dark];

    /// Stable string stored in the config.
    pub fn key(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::Dracula => "dracula",
        }
    }

    /// Parses a stored key; anything unknown falls back to `Dark`.
    pub fn from_key(key: &str) -> Theme {
        match key {
            "light" => Theme::Light,
            "dracula" => Theme::Dracula,
            _ => Theme::Dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark (default)",
            Theme::Light => "Light",
            Theme::Dracula => "Dracula",
        }
    }

    pub fn button_text(self) -> &'static str {
        match self {
            Theme::Dark => "DARK (DEFAULT)",
            Theme::Light => "LIGHT",
            Theme::Dracula => "DRACULA",
        }
    }
}

// Stored as its key string; an unknown key reads as Dark instead of
// failing the whole config.
impl Serialize for Theme {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for Theme {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Theme::from_key(&String::deserialize(d)?))
    }
}

#[derive(Clone, Copy)]
struct Palette {
    window_bg: Rgb,
    control_bg: Rgb,
    text_fg: Rgb,
    text_secondary: Rgb,
    primary: Rgb,
    success: Rgb,
    error: Rgb,
    neutral: Rgb,
    /// Dark title bar / dark control visual styles, vs the OS light ones.
    dark_chrome: bool,
}

const DARK_PALETTE: Palette = Palette {
    window_bg: (18, 18, 18),         // Material dark "background"
    control_bg: (32, 32, 35),        // Material dark "surface" (elevation 1)
    text_fg: (230, 230, 230),        // ~87% white, high-emphasis text
    text_secondary: (150, 150, 150), // ~60% white, medium-emphasis text
    primary: (138, 180, 248),        // Material blue 300
    success: (76, 175, 80),          // Material green 500
    error: (244, 67, 54),            // Material red 500
    neutral: (120, 120, 120),
    dark_chrome: true,
};

const LIGHT_PALETTE: Palette = Palette {
    window_bg: (245, 245, 245),
    control_bg: (255, 255, 255),
    text_fg: (33, 33, 33),
    text_secondary: (97, 97, 97),
    primary: (25, 118, 210), // Material blue 700
    success: (56, 142, 60),  // Material green 700
    error: (211, 47, 47),    // Material red 700
    neutral: (117, 117, 117),
    dark_chrome: false,
};

const DRACULA_PALETTE: Palette = Palette {
    window_bg: (40, 42, 54),         // #282a36 "Background"
    control_bg: (68, 71, 90),        // #44475a "Current Line"
    text_fg: (248, 248, 242),        // #f8f8f2 "Foreground"
    text_secondary: (139, 150, 194), // lightened "Comment"
    primary: (189, 147, 249),        // #bd93f9 "Purple"
    success: (80, 250, 123),         // #50fa7b "Green"
    error: (255, 85, 85),            // #ff5555 "Red"
    neutral: (98, 114, 164),         // #6272a4 "Comment"
    dark_chrome: true,
};

thread_local! {
    static CURRENT_THEME: Cell<Theme> = const { Cell::new(Theme::Dark) };
}

pub fn current_theme() -> Theme {
    CURRENT_THEME.with(Cell::get)
}

fn palette() -> Palette {
    match current_theme() {
        Theme::Dark => DARK_PALETTE,
        Theme::Light => LIGHT_PALETTE,
        Theme::Dracula => DRACULA_PALETTE,
    }
}

/// Switches the active theme and repaints every themed window and control.
/// Fine to call before any window exists.
pub fn set_theme(theme: Theme) {
    CURRENT_THEME.with(|c| c.set(theme));
    win32::repaint_for_theme_change();
}

pub fn window_bg() -> Rgb {
    palette().window_bg
}
pub fn control_bg() -> Rgb {
    palette().control_bg
}
pub fn text_fg() -> Rgb {
    palette().text_fg
}
pub fn text_secondary() -> Rgb {
    palette().text_secondary
}
pub fn primary() -> Rgb {
    palette().primary
}
pub fn success() -> Rgb {
    palette().success
}
pub fn error() -> Rgb {
    palette().error
}
pub fn neutral() -> Rgb {
    palette().neutral
}
fn dark_chrome() -> bool {
    palette().dark_chrome
}

/// Which palette slot an owner-drawn control is coloured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Primary,
    Success,
    Error,
}

impl Role {
    fn rgb(self) -> Rgb {
        match self {
            Role::Primary => primary(),
            Role::Success => success(),
            Role::Error => error(),
        }
    }
}

/// `COLORREF` for `color`.
fn colorref((r, g, b): Rgb) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

/// Linear interpolation from `base` towards `target` by `t` (0.0..=1.0),
/// for hover/pressed shades without alpha blending.
fn blend(base: Rgb, target: Rgb, t: f32) -> Rgb {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lerp = |b: u8, c: u8| (f32::from(b) + (f32::from(c) - f32::from(b)) * t).round().clamp(0.0, 255.0) as u8;
    (lerp(base.0, target.0), lerp(base.1, target.1), lerp(base.2, target.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for t in Theme::ALL {
            assert_eq!(Theme::from_key(t.key()), t);
        }
        assert_eq!(Theme::from_key("nope"), Theme::Dark);
    }

    #[test]
    fn serde_uses_keys() {
        assert_eq!(serde_json::to_string(&Theme::Dracula).unwrap(), "\"dracula\"");
        assert_eq!(serde_json::from_str::<Theme>("\"light\"").unwrap(), Theme::Light);
        assert_eq!(serde_json::from_str::<Theme>("\"???\"").unwrap(), Theme::Dark);
    }

    #[test]
    fn blend_endpoints_and_midpoint() {
        assert_eq!(blend((0, 0, 0), (200, 100, 50), 0.0), (0, 0, 0));
        assert_eq!(blend((0, 0, 0), (200, 100, 50), 1.0), (200, 100, 50));
        assert_eq!(blend((0, 0, 0), (200, 100, 50), 0.5), (100, 50, 25));
    }

    #[test]
    fn colorref_is_bgr() {
        assert_eq!(colorref((0x11, 0x22, 0x33)), 0x0033_2211);
    }
}
