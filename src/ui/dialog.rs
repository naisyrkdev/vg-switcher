//! Message boxes. Owned by the window the action came from, so that window
//! is disabled while the box is open. A hidden owner (e.g. an action started
//! from the tray) gets an ownerless box instead, so it can't end up hidden
//! too. `App::dispatch` ignores new actions while any box is open.

use native_windows_gui as nwg;

const TITLE: &str = "VG Switcher";

pub fn error(owner: &nwg::Window, text: &str) {
    log::warn!("error shown: {text}");
    if owner.visible() {
        nwg::modal_error_message(owner, TITLE, text);
    } else {
        nwg::error_message(TITLE, text);
    }
}

pub fn info(owner: &nwg::Window, text: &str) {
    if owner.visible() {
        nwg::modal_info_message(owner, TITLE, text);
    } else {
        nwg::simple_message(TITLE, text);
    }
}

/// Yes/No question with a warning icon. `true` for Yes.
pub fn confirm(owner: &nwg::Window, text: &str) -> bool {
    let params = nwg::MessageParams {
        title: TITLE,
        content: text,
        buttons: nwg::MessageButtons::YesNo,
        icons: nwg::MessageIcons::Warning,
    };
    let choice = if owner.visible() { nwg::modal_message(owner, &params) } else { nwg::message(&params) };
    choice == nwg::MessageChoice::Yes
}
