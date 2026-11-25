use native_windows_gui as nwg;

use crate::error::Result;

/// Fonts and the icon shared by every window.
pub struct Resources {
    pub mono_font: nwg::Font,
    pub bold_font: nwg::Font,
    pub app_icon: nwg::Icon,
}

fn font(family: &str, weight: u32) -> Result<nwg::Font> {
    let mut font = nwg::Font::default();
    nwg::Font::builder().family(family).size(18).weight(weight).build(&mut font)?;
    Ok(font)
}

impl Resources {
    /// Loads everything and makes Segoe UI the default font for new controls.
    pub fn load() -> Result<Self> {
        nwg::Font::set_global_default(Some(font("Segoe UI", 400)?));

        let mut app_icon = nwg::Icon::default();
        nwg::Icon::builder().source_bin(Some(include_bytes!("../../assets/icon.ico"))).build(&mut app_icon)?;

        Ok(Resources { mono_font: font("Consolas", 400)?, bold_font: font("Segoe UI", 700)?, app_icon })
    }
}
