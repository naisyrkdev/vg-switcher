#![windows_subsystem = "windows"]

mod config;
mod darkmode;
mod platform;
mod riot;
mod vanguard;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use native_windows_gui as nwg;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use config::Config;
use darkmode::{Role, Theme};
use platform::startup_apps::StartupStatus;
use vanguard::VanguardStatus;

// Shared layout metrics.
const MARGIN: i32 = 10;
const LABEL_H: i32 = 18;
const LABEL_GAP: i32 = 6;
const UNDERLINE_H: i32 = 3;
const GAP: i32 = 8;
const SECTION_GAP: i32 = 20;
const BUTTON_H: i32 = 32;
const SERVICE_INPUT_H: i32 = 24;
const TOGGLE_W: i32 = 44;
const TOGGLE_H: i32 = 24;
const STATUS_H: i32 = 28;

/// Full status text, used for the tray tooltip/menu where there's no
/// adjacent "Current status:" label to give it context.
fn status_text(status: VanguardStatus) -> &'static str {
    match status {
        VanguardStatus::Enabled => "Vanguard: ENABLED",
        VanguardStatus::Disabled => "Vanguard: DISABLED",
        VanguardStatus::NotInstalled => "Vanguard: NOT INSTALLED",
    }
}

/// Short status text for the on-screen label, which already sits right next
/// to a "Current status:" label.
fn status_short_text(status: VanguardStatus) -> &'static str {
    match status {
        VanguardStatus::Enabled => "ENABLED",
        VanguardStatus::Disabled => "DISABLED",
        VanguardStatus::NotInstalled => "NOT INSTALLED",
    }
}

fn status_color(status: VanguardStatus) -> (u8, u8, u8) {
    match status {
        VanguardStatus::Enabled => darkmode::success(),
        VanguardStatus::Disabled => darkmode::error(),
        VanguardStatus::NotInstalled => darkmode::neutral(),
    }
}

/// Status text for the Vanguard tray/app process row, which tracks whether
/// `vgtray.exe` is actually running right now - independent of (and often
/// stale relative to) the service start type above it.
fn tray_status_text(running: bool) -> &'static str {
    if running { "ENABLED" } else { "DISABLED" }
}

fn tray_status_color(running: bool) -> (u8, u8, u8) {
    if running { darkmode::success() } else { darkmode::error() }
}

/// Status text for Vanguard's own Windows "Startup Apps" entry.
fn startup_status_text(status: StartupStatus) -> &'static str {
    match status {
        StartupStatus::Enabled => "ENABLED",
        StartupStatus::Disabled => "DISABLED",
        StartupStatus::NotFound => "NOT FOUND",
    }
}


fn make_icon(status: VanguardStatus) -> Icon {
    let (r, g, b) = status_color(status);
    let size: u32 = 32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    let radius = size as f32 / 2.0 - 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center + 0.5;
            let dy = y as f32 - center + 0.5;
            if (dx * dx + dy * dy).sqrt() <= radius {
                let idx = ((y * size + x) * 4) as usize;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;
            }
        }
    }
    Icon::from_rgba(rgba, size, size).expect("failed to build tray icon")
}

fn show_error(message: &str) {
    nwg::error_message("VG Switcher", message);
}

fn show_info(message: &str) {
    nwg::simple_message("VG Switcher", message);
}

fn parse_commands(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

fn build_label(parent: &nwg::Window, text: &str, font: Option<&nwg::Font>) -> nwg::Label {
    let mut label = nwg::Label::default();
    nwg::Label::builder().text(text).parent(parent).font(font).build(&mut label).unwrap();
    label
}

fn build_button(parent: &nwg::Window, text: &str) -> nwg::Button {
    let mut button = nwg::Button::default();
    nwg::Button::builder().text(text).parent(parent).build(&mut button).unwrap();
    button
}

/// An owner-drawn on/off switch plus its caption label.
fn build_toggle(parent: &nwg::Window, caption: &str) -> (nwg::Button, nwg::Label) {
    let toggle = build_button(parent, "");
    if let Some(h) = toggle.handle.hwnd() {
        darkmode::make_dark_toggle(h);
    }
    (toggle, build_label(parent, caption, None))
}

/// Places a toggle and its caption on one row. The caption is kept exactly
/// one text line tall and vertically centered against the switch: nwg::Label
/// vertically centers by shrinking its client rect and painting the leftover
/// strip as non-client area in plain white (COLOR_WINDOW), so a label any
/// taller than its text shows up as white lines above and below it.
fn position_toggle_row(toggle: &nwg::Button, caption: &nwg::Label, y: i32, content_w: i32) {
    toggle.set_position(MARGIN, y);
    toggle.set_size(TOGGLE_W as u32, TOGGLE_H as u32);
    let caption_x = MARGIN + TOGGLE_W + 10;
    caption.set_position(caption_x, y + (TOGGLE_H - LABEL_H) / 2);
    caption.set_size((content_w - (caption_x - MARGIN)).max(60) as u32, LABEL_H as u32);
}

/// Places a "Prefix:" label followed by a value label right after the
/// prefix's actual rendered text width, both kept text-tight (see
/// `position_toggle_row` for why).
fn position_status_row(prefix: &nwg::Label, prefix_text: &str, value: &nwg::Label, y: i32, content_w: i32) {
    let offset = (STATUS_H - LABEL_H) / 2;
    let prefix_w = prefix.handle.hwnd().map(|h| darkmode::measure_text_width(h, prefix_text)).unwrap_or(140);
    prefix.set_position(MARGIN, y + offset);
    prefix.set_size((prefix_w + 4) as u32, LABEL_H as u32);
    let value_x = MARGIN + prefix_w + 10;
    value.set_position(value_x, y + offset);
    value.set_size((content_w - (value_x - MARGIN)).max(60) as u32, LABEL_H as u32);
}

/// One field's own Edit/Restore Default (view mode) and Save/Cancel (edit
/// mode) buttons, placed directly under that field.
struct EditControls {
    edit_btn: nwg::Button,
    restore_btn: nwg::Button,
    save_btn: nwg::Button,
    cancel_btn: nwg::Button,
}

impl EditControls {
    fn set_mode(&self, editing: bool) {
        self.edit_btn.set_visible(!editing);
        self.restore_btn.set_visible(!editing);
        self.save_btn.set_visible(editing);
        self.cancel_btn.set_visible(editing);
    }

    fn position_row(&self, x: i32, y: i32, button_h: u32) {
        self.edit_btn.set_position(x, y);
        self.edit_btn.set_size(100, button_h);
        self.save_btn.set_position(x, y);
        self.save_btn.set_size(100, button_h);
        self.restore_btn.set_position(x + 110, y);
        self.restore_btn.set_size(150, button_h);
        self.cancel_btn.set_position(x + 110, y);
        self.cancel_btn.set_size(100, button_h);
    }
}

fn build_edit_controls(window: &nwg::Window) -> EditControls {
    let edit_btn = build_button(window, "EDIT");
    let restore_btn = build_button(window, "RESTORE DEFAULT");
    let save_btn = build_button(window, "SAVE");
    save_btn.set_visible(false);
    let cancel_btn = build_button(window, "CANCEL");
    cancel_btn.set_visible(false);

    for hwnd in [edit_btn.handle.hwnd(), restore_btn.handle.hwnd(), save_btn.handle.hwnd(), cancel_btn.handle.hwnd()]
    {
        if let Some(h) = hwnd {
            darkmode::make_text_button(h, Role::Primary);
        }
    }

    EditControls { edit_btn, restore_btn, save_btn, cancel_btn }
}

/// Main window: status readouts, startup toggles and quick actions.
struct Ui {
    window: nwg::Window,
    autostart_toggle: nwg::Button,
    autostart_toggle_label: nwg::Label,
    vanguard_startup_toggle: nwg::Button,
    vanguard_startup_toggle_label: nwg::Label,
    status_prefix_label: nwg::Label,
    status_value_label: nwg::Label,
    tray_status_prefix_label: nwg::Label,
    tray_status_value_label: nwg::Label,
    utility_label: nwg::Label,
    kill_tray_btn: nwg::Button,
    force_kill_riot_btn: nwg::Button,
    reboot_btn: nwg::Button,
    vanguard_process_label: nwg::Label,
    enable_btn: nwg::Button,
    disable_btn: nwg::Button,
    commands_settings_btn: nwg::Button,
    general_settings_btn: nwg::Button,
    hide_btn: nwg::Button,
    minimize_btn: nwg::Button,
    close_btn: nwg::Button,
    timer: nwg::AnimationTimer,
}

/// The separate "Commands Settings" window: the enable/disable command lists
/// and the status-check service name, each with its own edit controls.
struct CommandsUi {
    window: nwg::Window,
    enable_label: nwg::Label,
    enable_box: nwg::TextBox,
    enable_underline: usize,
    enable_controls: EditControls,
    disable_label: nwg::Label,
    disable_box: nwg::TextBox,
    disable_underline: usize,
    disable_controls: EditControls,
    service_label: nwg::Label,
    service_input: nwg::TextInput,
    service_underline: usize,
    service_controls: EditControls,
    close_btn: nwg::Button,
}

/// The separate "General Settings" window - app-level preferences (today:
/// just the color theme).
struct SettingsUi {
    window: nwg::Window,
    // Never read, but owning it keeps the control alive (dropping an nwg
    // control destroys its window).
    _theme_label: nwg::Label,
    current_theme_label: nwg::Label,
    light_btn: nwg::Button,
    dracula_btn: nwg::Button,
    dark_btn: nwg::Button,
    close_btn: nwg::Button,
}

/// Shown after enabling Vanguard from a disabled state: the service start
/// types only take effect on the next boot.
struct RebootUi {
    window: nwg::Window,
    // Never read, but owning them keeps the controls alive.
    _message_labels: [nwg::Label; 2],
    reboot_now_btn: nwg::Button,
    reboot_later_btn: nwg::Button,
}

struct TrayHandles {
    icon: TrayIcon,
    status_item: MenuItem,
    enable_item: MenuItem,
    disable_item: MenuItem,
    kill_tray_item: MenuItem,
    settings_item: MenuItem,
    general_settings_item: MenuItem,
    refresh_item: MenuItem,
    quit_item: MenuItem,
}

struct AppState {
    config: RefCell<Config>,
    tray: TrayHandles,
    ui: Ui,
    commands_ui: CommandsUi,
    settings_ui: SettingsUi,
    reboot_ui: RebootUi,
    last_poll: Cell<Instant>,
    editing_enable: Cell<bool>,
    editing_disable: Cell<bool>,
    editing_service: Cell<bool>,
}

#[derive(Clone, Copy)]
enum Field {
    Enable,
    Disable,
    Service,
}

impl Field {
    fn controls<'a>(self, ui: &'a CommandsUi) -> &'a EditControls {
        match self {
            Field::Enable => &ui.enable_controls,
            Field::Disable => &ui.disable_controls,
            Field::Service => &ui.service_controls,
        }
    }

    fn editing_flag(self, app: &AppState) -> &Cell<bool> {
        match self {
            Field::Enable => &app.editing_enable,
            Field::Disable => &app.editing_disable,
            Field::Service => &app.editing_service,
        }
    }

    fn set_readonly(self, ui: &CommandsUi, readonly: bool) {
        match self {
            Field::Enable => ui.enable_box.set_readonly(readonly),
            Field::Disable => ui.disable_box.set_readonly(readonly),
            Field::Service => ui.service_input.set_readonly(readonly),
        }
    }

    fn focus(self, ui: &CommandsUi) {
        match self {
            Field::Enable => ui.enable_box.set_focus(),
            Field::Disable => ui.disable_box.set_focus(),
            Field::Service => ui.service_input.set_focus(),
        }
    }

    /// Writes this field's current on-screen text into `cfg`.
    fn save_into(self, ui: &CommandsUi, cfg: &mut Config) {
        match self {
            Field::Enable => cfg.enable_commands = parse_commands(&ui.enable_box.text()),
            Field::Disable => cfg.disable_commands = parse_commands(&ui.disable_box.text()),
            Field::Service => {
                let service = ui.service_input.text().trim().to_string();
                cfg.status_service = if service.is_empty() { "vgc".to_string() } else { service };
            }
        }
    }

    /// Sets this field's on-screen text from `cfg`.
    fn reload_from(self, ui: &CommandsUi, cfg: &Config) {
        match self {
            Field::Enable => ui.enable_box.set_text(&cfg.enable_commands.join("\r\n")),
            Field::Disable => ui.disable_box.set_text(&cfg.disable_commands.join("\r\n")),
            Field::Service => ui.service_input.set_text(&cfg.status_service),
        }
    }
}

fn set_field_editing_mode(app: &AppState, field: Field, editing: bool) {
    field.editing_flag(app).set(editing);
    field.set_readonly(&app.commands_ui, !editing);
    field.controls(&app.commands_ui).set_mode(editing);
    if editing {
        field.focus(&app.commands_ui);
    }
}

fn save_field(app: &AppState, field: Field) {
    field.save_into(&app.commands_ui, &mut app.config.borrow_mut());
    if let Err(e) = app.config.borrow().save() {
        show_error(&format!("Failed to save settings:\n{e}"));
    }
}

fn reload_field(app: &AppState, field: Field) {
    field.reload_from(&app.commands_ui, &app.config.borrow());
}

fn restore_field_default(app: &AppState, field: Field) {
    let defaults = Config::default();
    {
        let mut cfg = app.config.borrow_mut();
        match field {
            Field::Enable => cfg.enable_commands = defaults.enable_commands,
            Field::Disable => cfg.disable_commands = defaults.disable_commands,
            Field::Service => cfg.status_service = defaults.status_service,
        }
    }
    if let Err(e) = app.config.borrow().save() {
        show_error(&format!("Failed to save settings:\n{e}"));
    }
    reload_field(app, field);
}

/// Handles a click if `handle` belongs to one of `field`'s own
/// Edit/Save/Cancel/Restore Default buttons. Returns true if it did.
fn handle_field_click(app: &Rc<AppState>, field: Field, handle: nwg::ControlHandle) -> bool {
    let controls = field.controls(&app.commands_ui);
    if handle == controls.edit_btn.handle {
        set_field_editing_mode(app, field, true);
    } else if handle == controls.save_btn.handle {
        save_field(app, field);
        set_field_editing_mode(app, field, false);
        refresh_status(app);
    } else if handle == controls.cancel_btn.handle {
        reload_field(app, field);
        set_field_editing_mode(app, field, false);
    } else if handle == controls.restore_btn.handle {
        restore_field_default(app, field);
        refresh_status(app);
    } else {
        return false;
    }
    true
}

/// Reloads every command field that isn't mid-edit from the saved config.
fn load_commands_from_config(app: &AppState) {
    if !app.editing_enable.get() {
        reload_field(app, Field::Enable);
    }
    if !app.editing_disable.get() {
        reload_field(app, Field::Disable);
    }
    if !app.editing_service.get() {
        reload_field(app, Field::Service);
    }
}

fn sync_autostart_toggle(app: &AppState) {
    if let Some(h) = app.ui.autostart_toggle.handle.hwnd() {
        darkmode::set_control_state(h, platform::autostart::is_enabled());
    }
}

fn refresh_status(app: &AppState) {
    let service = app.config.borrow().status_service.clone();
    let status = vanguard::get_status(&service);
    let text = status_text(status);
    app.tray.status_item.set_text(text);
    let _ = app.tray.icon.set_icon(Some(make_icon(status)));
    let _ = app.tray.icon.set_tooltip(Some(text));

    // Set the color before the text so the repaint WM_SETTEXT triggers
    // already picks up the new color instead of a stale one.
    if let Some(h) = app.ui.status_value_label.handle.hwnd() {
        darkmode::set_static_text_color(h, status_color(status));
    }
    app.ui.status_value_label.set_text(status_short_text(status));

    let tray_running = vanguard::tray_running();
    if let Some(h) = app.ui.tray_status_value_label.handle.hwnd() {
        darkmode::set_static_text_color(h, tray_status_color(tray_running));
    }
    app.ui.tray_status_value_label.set_text(tray_status_text(tray_running));

    let startup_status = platform::startup_apps::status();
    app.ui.vanguard_startup_toggle.set_enabled(startup_status != StartupStatus::NotFound);
    if let Some(h) = app.ui.vanguard_startup_toggle.handle.hwnd() {
        darkmode::set_control_state(h, startup_status == StartupStatus::Enabled);
    }
}

/// Lays out the main window. Everything here is fixed-height, so it only
/// needs the width to stretch rows and pin the Close button right.
fn relayout(ui: &Ui) {
    let (w, _) = ui.window.size();
    let w = w as i32;
    let content_w = (w - MARGIN * 2).max(200);

    let mut y = MARGIN;

    position_toggle_row(&ui.autostart_toggle, &ui.autostart_toggle_label, y, content_w);
    y += TOGGLE_H + GAP;

    position_toggle_row(&ui.vanguard_startup_toggle, &ui.vanguard_startup_toggle_label, y, content_w);
    y += TOGGLE_H + SECTION_GAP;

    position_status_row(&ui.status_prefix_label, "Current status:", &ui.status_value_label, y, content_w);
    y += STATUS_H + GAP;

    position_status_row(
        &ui.tray_status_prefix_label,
        "Vanguard tray/app:",
        &ui.tray_status_value_label,
        y,
        content_w,
    );
    y += STATUS_H + SECTION_GAP;

    ui.vanguard_process_label.set_position(MARGIN, y);
    ui.vanguard_process_label.set_size(content_w as u32, LABEL_H as u32);
    y += LABEL_H + LABEL_GAP;

    ui.enable_btn.set_position(MARGIN, y);
    ui.enable_btn.set_size(110, BUTTON_H as u32);
    ui.disable_btn.set_position(MARGIN + 120, y);
    ui.disable_btn.set_size(110, BUTTON_H as u32);
    ui.commands_settings_btn.set_position(MARGIN + 240, y);
    ui.commands_settings_btn.set_size(180, BUTTON_H as u32);
    y += BUTTON_H + SECTION_GAP;

    ui.utility_label.set_position(MARGIN, y);
    ui.utility_label.set_size(content_w as u32, LABEL_H as u32);
    y += LABEL_H + LABEL_GAP;

    ui.kill_tray_btn.set_position(MARGIN, y);
    ui.kill_tray_btn.set_size(190, BUTTON_H as u32);
    ui.force_kill_riot_btn.set_position(MARGIN + 200, y);
    ui.force_kill_riot_btn.set_size(220, BUTTON_H as u32);
    ui.reboot_btn.set_position(MARGIN + 430, y);
    ui.reboot_btn.set_size(110, BUTTON_H as u32);
    ui.general_settings_btn.set_position(MARGIN + 550, y);
    ui.general_settings_btn.set_size(170, BUTTON_H as u32);
    y += BUTTON_H + SECTION_GAP;

    let close_x = (w - MARGIN - 80).max(MARGIN + 210);
    ui.close_btn.set_position(close_x, y);
    ui.close_btn.set_size(80, BUTTON_H as u32);
    ui.minimize_btn.set_position(close_x - 120, y);
    ui.minimize_btn.set_size(110, BUTTON_H as u32);
    ui.hide_btn.set_position(close_x - 210, y);
    ui.hide_btn.set_size(80, BUTTON_H as u32);
}

/// Lays out the Commands Settings window: the two command boxes share
/// whatever height is left after the fixed-height chrome, so they grow and
/// shrink with the window.
fn relayout_commands(ui: &CommandsUi) {
    let (w, h) = ui.window.size();
    let w = w as i32;
    let h = h as i32;
    let content_w = (w - MARGIN * 2).max(200);

    let fixed_h = (LABEL_H + LABEL_GAP + UNDERLINE_H + GAP + BUTTON_H + SECTION_GAP) * 2
        + (LABEL_H + LABEL_GAP + SERVICE_INPUT_H + UNDERLINE_H + GAP + BUTTON_H + SECTION_GAP)
        + BUTTON_H;
    let boxes_total = (h - MARGIN * 2 - fixed_h).max(120);
    let each_box_h = (boxes_total / 2).max(50);

    let mut y = MARGIN;

    ui.enable_label.set_position(MARGIN, y);
    ui.enable_label.set_size(content_w as u32, LABEL_H as u32);
    y += LABEL_H + LABEL_GAP;
    ui.enable_box.set_position(MARGIN, y);
    ui.enable_box.set_size(content_w as u32, each_box_h as u32);
    y += each_box_h;
    darkmode::set_accent_bar_rect(ui.enable_underline, MARGIN, y, content_w, UNDERLINE_H);
    y += UNDERLINE_H + GAP;
    ui.enable_controls.position_row(MARGIN, y, BUTTON_H as u32);
    y += BUTTON_H + SECTION_GAP;

    ui.disable_label.set_position(MARGIN, y);
    ui.disable_label.set_size(content_w as u32, LABEL_H as u32);
    y += LABEL_H + LABEL_GAP;
    ui.disable_box.set_position(MARGIN, y);
    ui.disable_box.set_size(content_w as u32, each_box_h as u32);
    y += each_box_h;
    darkmode::set_accent_bar_rect(ui.disable_underline, MARGIN, y, content_w, UNDERLINE_H);
    y += UNDERLINE_H + GAP;
    ui.disable_controls.position_row(MARGIN, y, BUTTON_H as u32);
    y += BUTTON_H + SECTION_GAP;

    ui.service_label.set_position(MARGIN, y);
    ui.service_label.set_size(content_w as u32, LABEL_H as u32);
    y += LABEL_H + LABEL_GAP;
    ui.service_input.set_position(MARGIN, y);
    ui.service_input.set_size(200, SERVICE_INPUT_H as u32);
    y += SERVICE_INPUT_H;
    darkmode::set_accent_bar_rect(ui.service_underline, MARGIN, y, 200, UNDERLINE_H);
    y += UNDERLINE_H + GAP;
    ui.service_controls.position_row(MARGIN, y, BUTTON_H as u32);
    y += BUTTON_H + SECTION_GAP;

    ui.close_btn.set_position((w - MARGIN - 80).max(MARGIN), y);
    ui.close_btn.set_size(80, BUTTON_H as u32);
}

fn show_settings(app: &Rc<AppState>) {
    sync_autostart_toggle(app);
    refresh_status(app);
    app.ui.window.set_visible(true);
    app.ui.window.restore();
    app.ui.window.set_focus();
}

fn show_commands_settings(app: &Rc<AppState>) {
    load_commands_from_config(app);
    app.commands_ui.window.set_visible(true);
    app.commands_ui.window.set_focus();
}

/// Updates the "Current theme:" readout and shows the General Settings
/// window.
fn show_general_settings(app: &Rc<AppState>) {
    let theme = Theme::from_key(&app.config.borrow().theme);
    app.settings_ui.current_theme_label.set_text(&format!("Current theme: {}", theme.label()));
    app.settings_ui.window.set_visible(true);
    app.settings_ui.window.set_focus();
}

/// Applies `theme`: switches the live palette, persists the choice, updates
/// the General Settings readout, and refreshes the main window's status
/// colors (which are set explicitly per-refresh rather than read from the
/// theme at paint time, so they'd otherwise stay stale after a switch).
fn apply_theme(app: &Rc<AppState>, theme: Theme) {
    darkmode::set_theme(theme);
    app.config.borrow_mut().theme = theme.key().to_string();
    if let Err(e) = app.config.borrow().save() {
        show_error(&format!("Failed to save settings:\n{e}"));
    }
    app.settings_ui.current_theme_label.set_text(&format!("Current theme: {}", theme.label()));
    refresh_status(app);
}

fn run_enable(app: &AppState) {
    let service = app.config.borrow().status_service.clone();
    let was_disabled = vanguard::get_status(&service) == VanguardStatus::Disabled;
    let cmds = app.config.borrow().enable_commands.clone();
    if let Err(e) = vanguard::run_commands(&cmds) {
        show_error(&format!("Failed to enable Vanguard:\n{e}"));
    }
    refresh_status(app);
    if was_disabled && vanguard::get_status(&service) == VanguardStatus::Enabled {
        app.reboot_ui.window.set_visible(true);
        app.reboot_ui.window.set_focus();
    }
}

fn run_disable(app: &AppState) {
    let cmds = app.config.borrow().disable_commands.clone();
    if let Err(e) = vanguard::run_commands(&cmds) {
        show_error(&format!("Failed to disable Vanguard:\n{e}"));
    }
    let _ = vanguard::kill_tray();
    refresh_status(app);
}

fn run_kill_tray(app: &AppState) {
    if let Err(e) = vanguard::kill_tray() {
        show_error(&format!("Failed to kill Vanguard tray:\n{e}"));
    }
    refresh_status(app);
}

/// Hides a window instead of destroying it when its close (X) is clicked.
fn hide_on_close(window: &nwg::Window, handle: nwg::ControlHandle, data: &nwg::EventData) {
    if handle == window.handle {
        if let nwg::EventData::OnWindowClose(close_data) = data {
            close_data.close(false);
        }
        window.set_visible(false);
    }
}

fn build_main_ui(bold_font: &nwg::Font, app_icon: &nwg::Icon) -> Ui {
    let mut window = nwg::Window::default();
    nwg::Window::builder()
        // Built hidden (no VISIBLE flag) even for a non-tray launch: the
        // window gets shown explicitly only once every control is built,
        // laid out and colored, instead of WS_VISIBLE making it appear
        // right away and letting the user catch it mid-construction (seen
        // as a flash of unstyled/mispositioned controls on slower starts).
        .flags(nwg::WindowFlags::MAIN_WINDOW)
        .size((760, 364))
        .position((300, 120))
        .title("VG Switcher")
        .icon(Some(app_icon))
        .build(&mut window)
        .unwrap();
    let hwnd = window.handle.hwnd().expect("window hwnd");
    darkmode::apply_titlebar_theme(hwnd);
    darkmode::install_dark_background(hwnd);

    let (autostart_toggle, autostart_toggle_label) = build_toggle(&window, "Start VG Switcher on logon");
    let (vanguard_startup_toggle, vanguard_startup_toggle_label) =
        build_toggle(&window, "Start Riot Vanguard at logon (same on/off as Windows' Startup Apps)");
    let status_prefix_label = build_label(&window, "Current status:", None);
    let status_value_label = build_label(&window, "", Some(bold_font));
    let tray_status_prefix_label = build_label(&window, "Vanguard tray/app:", None);
    let tray_status_value_label = build_label(&window, "", Some(bold_font));
    let utility_label = build_label(&window, "Utility:", None);
    let kill_tray_btn = build_button(&window, "KILL VANGUARD TRAY");
    let force_kill_riot_btn = build_button(&window, "FORCE KILL RIOT CLIENT");
    let reboot_btn = build_button(&window, "REBOOT");
    let vanguard_process_label = build_label(&window, "Vanguard Process:", None);
    let enable_btn = build_button(&window, "ENABLE NOW");
    let disable_btn = build_button(&window, "DISABLE NOW");
    let commands_settings_btn = build_button(&window, "COMMANDS SETTINGS");
    let general_settings_btn = build_button(&window, "GENERAL SETTINGS");
    let hide_btn = build_button(&window, "HIDE");
    let minimize_btn = build_button(&window, "MINIMIZE");
    let close_btn = build_button(&window, "CLOSE");

    for (btn, role) in [
        (&enable_btn, Role::Success),
        (&disable_btn, Role::Error),
        (&kill_tray_btn, Role::Error),
        (&force_kill_riot_btn, Role::Error),
        (&reboot_btn, Role::Error),
        (&close_btn, Role::Error),
    ] {
        if let Some(h) = btn.handle.hwnd() {
            darkmode::make_filled_button(h, role);
        }
    }
    for btn in [&commands_settings_btn, &general_settings_btn, &hide_btn, &minimize_btn] {
        if let Some(h) = btn.handle.hwnd() {
            darkmode::make_text_button(h, Role::Primary);
        }
    }

    let mut timer = nwg::AnimationTimer::default();
    nwg::AnimationTimer::builder()
        .parent(&window)
        .interval(Duration::from_millis(300))
        .max_tick(None)
        .lifetime(None)
        .active(true)
        .build(&mut timer)
        .unwrap();

    let ui = Ui {
        window,
        autostart_toggle,
        autostart_toggle_label,
        vanguard_startup_toggle,
        vanguard_startup_toggle_label,
        status_prefix_label,
        status_value_label,
        tray_status_prefix_label,
        tray_status_value_label,
        utility_label,
        kill_tray_btn,
        force_kill_riot_btn,
        reboot_btn,
        vanguard_process_label,
        enable_btn,
        disable_btn,
        commands_settings_btn,
        general_settings_btn,
        hide_btn,
        minimize_btn,
        close_btn,
        timer,
    };
    relayout(&ui);
    ui
}

fn build_commands_ui(mono_font: &nwg::Font, app_icon: &nwg::Icon) -> CommandsUi {
    let mut window = nwg::Window::default();
    nwg::Window::builder()
        .flags(nwg::WindowFlags::MAIN_WINDOW)
        .size((700, 640))
        .position((340, 100))
        .title("VG Switcher - Commands Settings")
        .icon(Some(app_icon))
        .build(&mut window)
        .unwrap();
    let hwnd = window.handle.hwnd().expect("commands window hwnd");
    darkmode::apply_titlebar_theme(hwnd);
    darkmode::install_dark_background(hwnd);

    let build_box = |window: &nwg::Window| {
        let mut text_box = nwg::TextBox::default();
        nwg::TextBox::builder()
            .parent(window)
            .font(Some(mono_font))
            .readonly(true)
            .build(&mut text_box)
            .unwrap();
        text_box
    };

    let enable_label = build_label(&window, "Enable commands (one per line):", None);
    let enable_box = build_box(&window);
    let enable_underline = darkmode::register_accent_bar(hwnd, Role::Primary);
    let enable_controls = build_edit_controls(&window);

    let disable_label = build_label(&window, "Disable commands (one per line):", None);
    let disable_box = build_box(&window);
    let disable_underline = darkmode::register_accent_bar(hwnd, Role::Primary);
    let disable_controls = build_edit_controls(&window);

    let service_label = build_label(&window, "Status-check service name:", None);
    let mut service_input = nwg::TextInput::default();
    nwg::TextInput::builder().parent(&window).readonly(true).build(&mut service_input).unwrap();
    let service_underline = darkmode::register_accent_bar(hwnd, Role::Primary);
    let service_controls = build_edit_controls(&window);

    let close_btn = build_button(&window, "CLOSE");
    if let Some(h) = close_btn.handle.hwnd() {
        darkmode::make_text_button(h, Role::Primary);
    }

    // Text fields: our own WM_CTLCOLOREDIT/WM_CTLCOLORSTATIC handling
    // covers background/text color, but newer Windows can still paint a
    // themed (light) background underneath a single-line Edit control
    // unless it's also opted into the dark visual-style variant - without
    // this, the service-name field renders with a stray light background.
    for ctrl_hwnd in [enable_box.handle.hwnd(), disable_box.handle.hwnd(), service_input.handle.hwnd()] {
        if let Some(h) = ctrl_hwnd {
            darkmode::apply_dark_control(h);
        }
    }

    let ui = CommandsUi {
        window,
        enable_label,
        enable_box,
        enable_underline,
        enable_controls,
        disable_label,
        disable_box,
        disable_underline,
        disable_controls,
        service_label,
        service_input,
        service_underline,
        service_controls,
        close_btn,
    };
    relayout_commands(&ui);
    ui
}

fn build_settings_ui(bold_font: &nwg::Font, app_icon: &nwg::Icon) -> SettingsUi {
    let mut window = nwg::Window::default();
    nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((360, 260))
        .position((360, 160))
        .title("VG Switcher - General Settings")
        .icon(Some(app_icon))
        .build(&mut window)
        .unwrap();
    let hwnd = window.handle.hwnd().expect("settings window hwnd");
    darkmode::apply_titlebar_theme(hwnd);
    darkmode::install_dark_background(hwnd);

    let theme_label = build_label(&window, "Theme:", None);
    theme_label.set_position(MARGIN, MARGIN);
    theme_label.set_size(320, LABEL_H as u32);

    let current_theme_label = build_label(&window, "", Some(bold_font));
    current_theme_label.set_position(MARGIN, 34);
    current_theme_label.set_size(320, LABEL_H as u32);

    let light_btn = build_button(&window, "LIGHT");
    let dracula_btn = build_button(&window, "DRACULA");
    let dark_btn = build_button(&window, "DARK (DEFAULT)");
    let close_btn = build_button(&window, "CLOSE");
    for (btn, y) in [(&light_btn, 66), (&dracula_btn, 108), (&dark_btn, 150)] {
        btn.set_position(MARGIN, y);
        btn.set_size(320, 34);
    }
    close_btn.set_position(250, 204);
    close_btn.set_size(80, BUTTON_H as u32);
    for btn in [&light_btn, &dracula_btn, &dark_btn, &close_btn] {
        if let Some(h) = btn.handle.hwnd() {
            darkmode::make_text_button(h, Role::Primary);
        }
    }

    SettingsUi { window, _theme_label: theme_label, current_theme_label, light_btn, dracula_btn, dark_btn, close_btn }
}

fn build_reboot_ui(app_icon: &nwg::Icon) -> RebootUi {
    let mut window = nwg::Window::default();
    nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((460, 150))
        .position((400, 220))
        .title("VG Switcher - Reboot required")
        .icon(Some(app_icon))
        .build(&mut window)
        .unwrap();
    let hwnd = window.handle.hwnd().expect("reboot window hwnd");
    darkmode::apply_titlebar_theme(hwnd);
    darkmode::install_dark_background(hwnd);

    // One single-line label per line, each exactly LABEL_H tall: see
    // AGENTS.md - an nwg::Label taller than its text paints white bands.
    let message_labels = [
        build_label(&window, "Vanguard has been enabled.", None),
        build_label(&window, "To make it take effect you need to reboot your computer.", None),
    ];
    for (i, label) in message_labels.iter().enumerate() {
        label.set_position(MARGIN, MARGIN + 10 + i as i32 * (LABEL_H + LABEL_GAP));
        label.set_size(440, LABEL_H as u32);
    }

    let reboot_now_btn = build_button(&window, "REBOOT NOW");
    let reboot_later_btn = build_button(&window, "REBOOT LATER (CLOSE THIS WINDOW)");
    reboot_now_btn.set_position(MARGIN, 100);
    reboot_now_btn.set_size(130, BUTTON_H as u32);
    reboot_later_btn.set_position(MARGIN + 140, 100);
    reboot_later_btn.set_size(300, BUTTON_H as u32);
    if let Some(h) = reboot_now_btn.handle.hwnd() {
        darkmode::make_filled_button(h, Role::Error);
    }
    if let Some(h) = reboot_later_btn.handle.hwnd() {
        darkmode::make_text_button(h, Role::Primary);
    }

    RebootUi { window, _message_labels: message_labels, reboot_now_btn, reboot_later_btn }
}

fn build_tray() -> TrayHandles {
    let status_item = MenuItem::new(status_text(VanguardStatus::NotInstalled), false, None);
    let enable_item = MenuItem::new("Enable Vanguard", true, None);
    let disable_item = MenuItem::new("Disable Vanguard", true, None);
    let kill_tray_item = MenuItem::new("Kill Vanguard Tray", true, None);
    let settings_item = MenuItem::new("Settings...", true, None);
    let general_settings_item = MenuItem::new("General Settings...", true, None);
    let refresh_item = MenuItem::new("Refresh", true, None);
    let quit_item = MenuItem::new("Exit", true, None);

    let tray_menu = Menu::new();
    tray_menu
        .append_items(&[
            &status_item,
            &PredefinedMenuItem::separator(),
            &enable_item,
            &disable_item,
            &kill_tray_item,
            &PredefinedMenuItem::separator(),
            &settings_item,
            &general_settings_item,
            &refresh_item,
            &PredefinedMenuItem::separator(),
            &quit_item,
        ])
        .expect("failed to build tray menu");

    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("VG Switcher")
        .with_icon(make_icon(VanguardStatus::NotInstalled))
        .build()
        .expect("failed to create tray icon");

    TrayHandles {
        icon,
        status_item,
        enable_item,
        disable_item,
        kill_tray_item,
        settings_item,
        general_settings_item,
        refresh_item,
        quit_item,
    }
}

fn main() {
    // The scheduled autostart task launches with --tray so it starts
    // hidden in the tray; a manual/double-click launch shows the window
    // right away instead of making the user dig for it in the tray menu.
    let start_hidden = std::env::args().any(|a| a == platform::autostart::TRAY_ARG);

    let config = Config::load();

    nwg::init().expect("failed to init native-windows-gui");
    darkmode::enable_app_dark_mode();
    darkmode::set_theme(Theme::from_key(&config.theme));

    let mut ui_font = nwg::Font::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(18)
        .build(&mut ui_font)
        .expect("failed to load UI font");
    nwg::Font::set_global_default(Some(ui_font));

    let mut mono_font = nwg::Font::default();
    nwg::Font::builder()
        .family("Consolas")
        .size(18)
        .build(&mut mono_font)
        .expect("failed to load monospace font");

    let mut bold_font = nwg::Font::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(18)
        .weight(700)
        .build(&mut bold_font)
        .expect("failed to load bold font");

    let mut app_icon = nwg::Icon::default();
    nwg::Icon::builder()
        .source_bin(Some(&include_bytes!("../assets/icon.ico")[..]))
        .build(&mut app_icon)
        .expect("failed to load app icon");

    let ui = build_main_ui(&bold_font, &app_icon);
    let commands_ui = build_commands_ui(&mono_font, &app_icon);
    let settings_ui = build_settings_ui(&bold_font, &app_icon);
    let reboot_ui = build_reboot_ui(&app_icon);
    let tray = build_tray();

    let app = Rc::new(AppState {
        config: RefCell::new(config),
        tray,
        ui,
        commands_ui,
        settings_ui,
        reboot_ui,
        last_poll: Cell::new(Instant::now()),
        editing_enable: Cell::new(false),
        editing_disable: Cell::new(false),
        editing_service: Cell::new(false),
    });

    load_commands_from_config(&app);
    sync_autostart_toggle(&app);
    refresh_status(&app);

    // Vanguard's own tray app can be left running from before a reboot even
    // though the service is already configured disabled - clear it out on
    // launch rather than making the user notice and hit "Disable" again.
    let startup_service = app.config.borrow().status_service.clone();
    if vanguard::get_status(&startup_service) == VanguardStatus::Disabled {
        let _ = vanguard::kill_tray();
        refresh_status(&app);
    }

    // Everything is built, laid out and colored now - safe to actually show
    // the window (it was created without WS_VISIBLE for exactly this).
    app.ui.window.set_visible(!start_hidden);

    let main_handle = app.ui.window.handle;
    let app_main = app.clone();
    let main_handler = nwg::full_bind_event_handler(&main_handle, move |evt, data, handle| {
        use nwg::Event as E;
        let app = &app_main;
        match evt {
            E::OnButtonClick => {
                if handle == app.ui.close_btn.handle {
                    app.ui.window.set_visible(false);
                } else if handle == app.ui.hide_btn.handle {
                    app.ui.window.set_visible(false);
                } else if handle == app.ui.minimize_btn.handle {
                    app.ui.window.minimize();
                } else if handle == app.ui.enable_btn.handle {
                    run_enable(app);
                } else if handle == app.ui.disable_btn.handle {
                    run_disable(app);
                } else if handle == app.ui.kill_tray_btn.handle {
                    run_kill_tray(app);
                } else if handle == app.ui.force_kill_riot_btn.handle {
                    match riot::force_kill_client() {
                        Ok(msg) => show_info(&msg),
                        Err(e) => show_error(&e),
                    }
                    refresh_status(app);
                } else if handle == app.ui.reboot_btn.handle {
                    let choice = nwg::modal_message(
                        &app.ui.window,
                        &nwg::MessageParams {
                            title: "VG Switcher",
                            content: "This will restart your computer in a few seconds.\n\nSave any work first. Reboot now?",
                            buttons: nwg::MessageButtons::YesNo,
                            icons: nwg::MessageIcons::Warning,
                        },
                    );
                    if choice == nwg::MessageChoice::Yes {
                        if let Err(e) = platform::process::reboot_now() {
                            show_error(&format!("Failed to schedule a reboot:\n{e}"));
                        }
                    }
                } else if handle == app.ui.commands_settings_btn.handle {
                    show_commands_settings(app);
                } else if handle == app.ui.general_settings_btn.handle {
                    show_general_settings(app);
                } else if handle == app.ui.autostart_toggle.handle {
                    let hwnd = app.ui.autostart_toggle.handle.hwnd();
                    let want_enabled = hwnd.map(|h| !darkmode::control_state(h)).unwrap_or(false);
                    let result = if want_enabled {
                        platform::autostart::enable()
                    } else {
                        platform::autostart::disable()
                    };
                    if let Err(e) = result {
                        show_error(&format!("Failed to update startup task:\n{e}"));
                    }
                    sync_autostart_toggle(app);
                } else if handle == app.ui.vanguard_startup_toggle.handle {
                    let hwnd = app.ui.vanguard_startup_toggle.handle.hwnd();
                    let want_enabled = hwnd.map(|h| !darkmode::control_state(h)).unwrap_or(false);
                    if let Err(e) = platform::startup_apps::set_enabled(want_enabled) {
                        show_error(&format!("Failed to update Vanguard's startup entry:\n{e}"));
                    } else {
                        let actual = platform::startup_apps::status();
                        let want = if want_enabled { StartupStatus::Enabled } else { StartupStatus::Disabled };
                        if actual != want {
                            show_error(&format!(
                                "Tried to {} Vanguard's startup entry, but it still reads as {}.\n\n\
                                 Check Settings > Apps > Startup to see what Windows reports.",
                                if want_enabled { "enable" } else { "disable" },
                                startup_status_text(actual),
                            ));
                        }
                    }
                    refresh_status(app);
                }
            }
            E::OnResize => {
                if handle == app.ui.window.handle {
                    relayout(&app.ui);
                }
            }
            E::OnMinMaxInfo => {
                if handle == app.ui.window.handle {
                    if let nwg::EventData::OnMinMaxInfo(info) = &data {
                        info.set_min_size(770, 405);
                    }
                }
            }
            E::OnWindowClose => hide_on_close(&app.ui.window, handle, &data),
            E::OnTimerTick => {
                if handle == app.ui.timer.handle {
                    while let Ok(event) = MenuEvent::receiver().try_recv() {
                        if event.id == app.tray.enable_item.id() {
                            run_enable(app);
                        } else if event.id == app.tray.disable_item.id() {
                            run_disable(app);
                        } else if event.id == app.tray.kill_tray_item.id() {
                            run_kill_tray(app);
                        } else if event.id == app.tray.settings_item.id() {
                            show_settings(app);
                        } else if event.id == app.tray.general_settings_item.id() {
                            show_general_settings(app);
                        } else if event.id == app.tray.refresh_item.id() {
                            refresh_status(app);
                        } else if event.id == app.tray.quit_item.id() {
                            nwg::stop_thread_dispatch();
                        }
                    }

                    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                        if let TrayIconEvent::DoubleClick { .. } = event {
                            show_settings(app);
                        }
                    }

                    let now = Instant::now();
                    if now.duration_since(app.last_poll.get()) >= Duration::from_secs(5) {
                        app.last_poll.set(now);
                        refresh_status(app);
                    }
                }
            }
            _ => {}
        }
    });

    let commands_handle = app.commands_ui.window.handle;
    let app_commands = app.clone();
    let commands_handler = nwg::full_bind_event_handler(&commands_handle, move |evt, data, handle| {
        use nwg::Event as E;
        let app = &app_commands;
        match evt {
            E::OnButtonClick => {
                if handle_field_click(app, Field::Enable, handle)
                    || handle_field_click(app, Field::Disable, handle)
                    || handle_field_click(app, Field::Service, handle)
                {
                } else if handle == app.commands_ui.close_btn.handle {
                    app.commands_ui.window.set_visible(false);
                }
            }
            E::OnResize => {
                if handle == app.commands_ui.window.handle {
                    relayout_commands(&app.commands_ui);
                }
            }
            E::OnMinMaxInfo => {
                if handle == app.commands_ui.window.handle {
                    if let nwg::EventData::OnMinMaxInfo(info) = &data {
                        info.set_min_size(600, 560);
                    }
                }
            }
            E::OnWindowClose => hide_on_close(&app.commands_ui.window, handle, &data),
            _ => {}
        }
    });

    let settings_handle = app.settings_ui.window.handle;
    let app_settings = app.clone();
    let settings_handler = nwg::full_bind_event_handler(&settings_handle, move |evt, data, handle| {
        use nwg::Event as E;
        let app = &app_settings;
        match evt {
            E::OnButtonClick => {
                if handle == app.settings_ui.light_btn.handle {
                    apply_theme(app, Theme::Light);
                } else if handle == app.settings_ui.dracula_btn.handle {
                    apply_theme(app, Theme::Dracula);
                } else if handle == app.settings_ui.dark_btn.handle {
                    apply_theme(app, Theme::Dark);
                } else if handle == app.settings_ui.close_btn.handle {
                    app.settings_ui.window.set_visible(false);
                }
            }
            E::OnWindowClose => hide_on_close(&app.settings_ui.window, handle, &data),
            _ => {}
        }
    });

    let reboot_handle = app.reboot_ui.window.handle;
    let app_reboot = app.clone();
    let reboot_handler = nwg::full_bind_event_handler(&reboot_handle, move |evt, data, handle| {
        use nwg::Event as E;
        let app = &app_reboot;
        match evt {
            E::OnButtonClick => {
                if handle == app.reboot_ui.reboot_now_btn.handle {
                    app.reboot_ui.window.set_visible(false);
                    if let Err(e) = platform::process::reboot_now() {
                        show_error(&format!("Failed to schedule a reboot:\n{e}"));
                    }
                } else if handle == app.reboot_ui.reboot_later_btn.handle {
                    app.reboot_ui.window.set_visible(false);
                }
            }
            E::OnWindowClose => hide_on_close(&app.reboot_ui.window, handle, &data),
            _ => {}
        }
    });

    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&main_handler);
    nwg::unbind_event_handler(&commands_handler);
    nwg::unbind_event_handler(&settings_handler);
    nwg::unbind_event_handler(&reboot_handler);
}
