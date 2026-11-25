//! Application state, startup, event routing and the action dispatcher.

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};

use native_windows_gui as nwg;

use crate::action::{Action, FieldOp};
use crate::config::{Config, ConfigStore};
use crate::error::Result;
use crate::platform::startup_apps::{self, StartupStatus};
use crate::platform::{autostart, paths, power, security, single_instance};
use crate::theme::{self, Theme};
use crate::ui::commands_window::CommandsWindow;
use crate::ui::main_window::MainWindow;
use crate::ui::reboot_window::RebootWindow;
use crate::ui::resources::Resources;
use crate::ui::settings_window::SettingsWindow;
use crate::ui::status_view::StatusView;
use crate::ui::tray::{self, Tray, TrayInput};
use crate::ui::{self, dialog};
use crate::vanguard::{self, Snapshot, VanguardStatus};
use crate::{logging, riot};

/// Raw-handler id for the main window's custom messages (must be > 0xFFFF).
const RAW_HANDLER_ID: usize = 0x5647_0001;

pub fn run() {
    // The autostart task passes --tray to start hidden; a manual launch
    // shows the window right away.
    let start_hidden = std::env::args().any(|a| a == autostart::TRAY_ARG);

    let Some(_instance) = single_instance::acquire() else {
        if !start_hidden {
            single_instance::ask_running_instance_to_show();
        }
        return;
    };

    let data_dir = paths::data_dir();
    if let Err(e) = security::ensure_secure_dir(&data_dir) {
        nwg::error_message("VG Switcher", &format!("Could not prepare the settings folder:\n{e}"));
        return;
    }
    logging::init(&data_dir);
    logging::install_panic_hook();
    log::info!("VG Switcher {} starting (hidden: {start_hidden})", env!("CARGO_PKG_VERSION"));

    if let Err(e) = start(ConfigStore::in_dir(&data_dir), start_hidden) {
        log::error!("startup failed: {e}");
        nwg::error_message("VG Switcher", &format!("VG Switcher could not start:\n{e}"));
    }
    log::info!("VG Switcher exiting");
}

fn start(store: ConfigStore, start_hidden: bool) -> Result<()> {
    let loaded = store.load(paths::legacy_config_file().as_deref());

    nwg::init()?;
    theme::enable_app_dark_mode();
    theme::set_theme(loaded.config.theme);

    let res = Resources::load()?;
    let (tray_tx, tray_rx) = mpsc::channel();
    let app = Rc::new(App {
        main: MainWindow::build(&res)?,
        commands: CommandsWindow::build(&res)?,
        settings: SettingsWindow::build(&res)?,
        reboot: RebootWindow::build(&res)?,
        tray: Tray::build()?,
        tray_input: tray_rx,
        store,
        config: RefCell::new(loaded.config),
        busy: Cell::new(false),
    });
    tray::forward_events(widgets_hwnd(&app.main.window), tray_tx);

    app.initial_sync();
    // Built hidden; shown only now that everything is laid out and coloured.
    app.main.window.set_visible(!start_hidden);
    if let Some(notice) = loaded.notice {
        dialog::info(&app.main.window, &notice);
    }

    let handlers = bind_events(&app)?;
    nwg::dispatch_thread_events();
    handlers.unbind();
    Ok(())
}

fn widgets_hwnd(window: &nwg::Window) -> winapi::shared::windef::HWND {
    ui::widgets::hwnd(&window.handle)
}

struct App {
    main: MainWindow,
    commands: CommandsWindow,
    settings: SettingsWindow,
    reboot: RebootWindow,
    tray: Tray,
    tray_input: Receiver<TrayInput>,
    store: ConfigStore,
    config: RefCell<Config>,
    /// Set while an action runs. A message box inside an action pumps
    /// messages, so without this guard clicks and timer ticks could start a
    /// second action in the middle of the first.
    busy: Cell<bool>,
}

/// Clears `App::busy` when the action ends, even by panic.
struct BusyGuard<'a>(&'a Cell<bool>);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl App {
    fn config(&self) -> Ref<'_, Config> {
        self.config.borrow()
    }

    /// Applies `change` to a copy of the config and saves it; the live
    /// config is replaced only if both succeed. No borrow is held while
    /// saving or while the caller shows any message.
    fn update_config(&self, change: impl FnOnce(&mut Config) -> Result<()>) -> Result<()> {
        let mut next = self.config().clone();
        change(&mut next)?;
        self.store.save(&next)?;
        *self.config.borrow_mut() = next;
        Ok(())
    }

    fn status_service(&self) -> String {
        self.config().status_service.clone()
    }

    fn initial_sync(&self) {
        self.commands.show_config(&self.config());
        self.settings.show_theme(self.config().theme);
        self.main.show_autostart(autostart::is_enabled());
        let snapshot = self.refresh();

        // Vanguard's tray app can survive from before a reboot even though
        // the service is already disabled; clear it out on launch.
        if snapshot.service == VanguardStatus::Disabled && snapshot.tray_running {
            if let Err(e) = vanguard::kill_tray() {
                log::warn!("killing the leftover Vanguard tray failed: {e}");
            }
            self.refresh();
        }
    }

    /// Re-reads every status and updates the main window and the tray.
    fn refresh(&self) -> Snapshot {
        let snapshot = Snapshot::read(&self.status_service());
        self.main.show_status(&snapshot);
        self.tray.show_status(snapshot.service);
        snapshot
    }

    /// Runs one action. Ignored while another action is still running (see
    /// `busy`).
    fn dispatch(&self, action: Action) {
        if self.busy.replace(true) {
            log::debug!("ignored {action:?}: another action is still running");
            return;
        }
        let _guard = BusyGuard(&self.busy);
        log::debug!("action {action:?}");

        match action {
            Action::EnableVanguard => self.enable_vanguard(),
            Action::DisableVanguard => self.disable_vanguard(),
            Action::KillVanguardTray => {
                if let Err(e) = vanguard::kill_tray() {
                    dialog::error(&self.main.window, &format!("Failed to kill the Vanguard tray:\n{e}"));
                }
                self.refresh();
            }
            Action::ForceKillRiotClient => {
                let report = riot::force_kill_client();
                if report.is_success() {
                    dialog::info(&self.main.window, &report.message());
                } else {
                    dialog::error(&self.main.window, &report.message());
                }
                self.refresh();
            }
            Action::Reboot => {
                if dialog::confirm(
                    &self.main.window,
                    "This will restart your computer in a few seconds.\n\nSave any work first. Reboot now?",
                ) {
                    self.reboot_now(&self.main.window);
                }
            }
            Action::ToggleAutostart => self.toggle_autostart(),
            Action::ToggleVanguardStartup => self.toggle_vanguard_startup(),

            Action::ShowMain => {
                self.main.show_autostart(autostart::is_enabled());
                self.refresh();
                ui::present(&self.main.window);
            }
            Action::HideMain => self.main.window.set_visible(false),
            Action::MinimizeMain => self.main.window.minimize(),
            Action::ShowCommands => {
                self.commands.show_config(&self.config());
                ui::present(&self.commands.window);
            }
            Action::HideCommands => self.commands.window.set_visible(false),
            Action::ShowGeneralSettings => {
                self.settings.show_theme(self.config().theme);
                ui::present(&self.settings.window);
            }
            Action::HideGeneralSettings => self.settings.window.set_visible(false),
            Action::SetTheme(theme) => self.set_theme(theme),

            Action::RebootFromPrompt => {
                self.reboot.window.set_visible(false);
                self.reboot_now(&self.main.window);
            }
            Action::DismissRebootPrompt => self.reboot.window.set_visible(false),

            Action::Field(index, op) => self.field_action(index, op),

            Action::Refresh => {
                self.refresh();
            }
            Action::Quit => nwg::stop_thread_dispatch(),
        }
    }

    fn enable_vanguard(&self) {
        let service = self.status_service();
        let was_disabled = vanguard::get_status(&service) == VanguardStatus::Disabled;
        let commands = self.config().enable_commands.clone();
        if let Err(e) = vanguard::run_commands(&commands) {
            dialog::error(&self.main.window, &format!("Failed to enable Vanguard:\n{e}"));
        }
        let now = self.refresh();
        if was_disabled && now.service == VanguardStatus::Enabled {
            ui::present(&self.reboot.window);
        }
    }

    fn disable_vanguard(&self) {
        let commands = self.config().disable_commands.clone();
        if let Err(e) = vanguard::run_commands(&commands) {
            dialog::error(&self.main.window, &format!("Failed to disable Vanguard:\n{e}"));
        }
        if let Err(e) = vanguard::kill_tray() {
            log::warn!("killing the Vanguard tray after disabling failed: {e}");
        }
        self.refresh();
    }

    fn reboot_now(&self, owner: &nwg::Window) {
        if let Err(e) = power::reboot_now() {
            dialog::error(owner, &format!("Failed to schedule a reboot:\n{e}"));
        }
    }

    fn toggle_autostart(&self) {
        let want = !self.main.autostart_is_on();
        let proceed = !want
            || paths::exe_in_protected_location()
            || dialog::confirm(
                &self.main.window,
                "VG Switcher is running from a folder outside Program Files, which programs running \
                 under your account can usually modify.\n\n\
                 Autostart runs this exact file with administrator rights at every logon, so any program \
                 that replaces it would get those rights too. Installing VG Switcher with the setup \
                 program avoids this.\n\nEnable autostart from this location anyway?",
            );
        if proceed {
            let result = if want { autostart::enable() } else { autostart::disable() };
            if let Err(e) = result {
                dialog::error(&self.main.window, &format!("Failed to update the startup task:\n{e}"));
            }
        }
        self.main.show_autostart(autostart::is_enabled());
    }

    fn toggle_vanguard_startup(&self) {
        let want = !self.main.vanguard_startup_is_on();
        match startup_apps::set_enabled(want) {
            Err(e) => dialog::error(&self.main.window, &format!("Failed to update Vanguard's startup entry:\n{e}")),
            Ok(()) => {
                let actual = startup_apps::status();
                let expected = if want { StartupStatus::Enabled } else { StartupStatus::Disabled };
                if actual != expected {
                    dialog::error(
                        &self.main.window,
                        &format!(
                            "Tried to {} Vanguard's startup entry, but it still reads as {}.\n\n\
                             Check Settings > Apps > Startup to see what Windows reports.",
                            if want { "enable" } else { "disable" },
                            actual.label(),
                        ),
                    );
                }
            }
        }
        self.refresh();
    }

    /// Switches the live theme, saves it, and re-colours the explicitly
    /// coloured status readouts and the tray icon.
    fn set_theme(&self, theme: Theme) {
        theme::set_theme(theme);
        if let Err(e) = self.update_config(|c| {
            c.theme = theme;
            Ok(())
        }) {
            dialog::error(&self.settings.window, &format!("Failed to save settings:\n{e}"));
        }
        self.settings.show_theme(theme);
        self.refresh();
    }

    fn field_action(&self, index: usize, op: FieldOp) {
        let Some(field) = self.commands.fields.get(index) else {
            return;
        };
        let owner = &self.commands.window;
        match op {
            FieldOp::Edit => field.set_editing(true),
            FieldOp::Cancel => {
                field.set_editing(false);
                field.show(&self.config());
            }
            FieldOp::Save => {
                let text = field.text();
                match self.update_config(|c| (field.spec.write)(c, &text)) {
                    // Stay in edit mode so the input isn't lost.
                    Err(e) => dialog::error(owner, &format!("Couldn't save \"{}\":\n{e}", field.spec.title)),
                    Ok(()) => {
                        field.set_editing(false);
                        field.show(&self.config());
                        self.refresh();
                    }
                }
            }
            FieldOp::RestoreDefault => {
                let default_text = (field.spec.read)(&Config::default());
                if let Err(e) = self.update_config(|c| (field.spec.write)(c, &default_text)) {
                    dialog::error(owner, &format!("Failed to save settings:\n{e}"));
                }
                field.show(&self.config());
                self.refresh();
            }
        }
    }

    /// Timer tick: keep the status current. Skipped while an action runs.
    fn poll(&self) {
        if !self.busy.get() {
            self.refresh();
        }
    }

    fn drain_tray_input(&self) {
        while let Ok(input) = self.tray_input.try_recv() {
            let action = match input {
                TrayInput::DoubleClick => Some(Action::ShowMain),
                TrayInput::Menu(id) => self.tray.action_for(&id),
            };
            if let Some(action) = action {
                self.dispatch(action);
            }
        }
    }

    fn action_for_button(&self, handle: nwg::ControlHandle) -> Option<Action> {
        self.main
            .action_for(handle)
            .or_else(|| self.commands.action_for(handle))
            .or_else(|| self.settings.action_for(handle))
            .or_else(|| self.reboot.action_for(handle))
    }

    fn on_event(&self, event: nwg::Event, data: &nwg::EventData, handle: nwg::ControlHandle) {
        use nwg::Event as E;
        match event {
            E::OnButtonClick => {
                if let Some(action) = self.action_for_button(handle) {
                    self.dispatch(action);
                }
            }
            E::OnWindowClose => {
                let _ = ui::hide_on_close(&self.main.window, handle, data)
                    || ui::hide_on_close(&self.commands.window, handle, data)
                    || ui::hide_on_close(&self.settings.window, handle, data)
                    || ui::hide_on_close(&self.reboot.window, handle, data);
            }
            E::OnResize if handle == self.main.window.handle => self.main.layout(),
            E::OnResize if handle == self.commands.window.handle => self.commands.layout(),
            E::OnMinMaxInfo if handle == self.main.window.handle => {
                ui::enforce_min_client_size(&self.main.window, data, MainWindow::MIN_CLIENT);
            }
            E::OnMinMaxInfo if handle == self.commands.window.handle => {
                ui::enforce_min_client_size(&self.commands.window, data, CommandsWindow::MIN_CLIENT);
            }
            E::OnTimerTick if handle == self.main.poll_timer.handle => self.poll(),
            _ => {}
        }
    }

    /// Custom messages to the main window: tray input waiting, or another
    /// launch asking us to show the window.
    fn on_raw_message(&self, msg: u32) -> Option<isize> {
        if msg == tray::WM_TRAY_INPUT {
            self.drain_tray_input();
            Some(0)
        } else if msg == single_instance::show_request_message() {
            self.dispatch(Action::ShowMain);
            Some(0)
        } else {
            None
        }
    }
}

struct Handlers {
    events: Vec<nwg::EventHandler>,
    raw: nwg::RawEventHandler,
}

impl Handlers {
    fn unbind(self) {
        for handler in &self.events {
            nwg::unbind_event_handler(handler);
        }
        if let Err(e) = nwg::unbind_raw_event_handler(&self.raw) {
            log::warn!("unbinding the raw handler failed: {e}");
        }
    }
}

/// Binds every window's events to `App::on_event`. The handlers keep the
/// `App` alive until `Handlers::unbind`.
fn bind_events(app: &Rc<App>) -> Result<Handlers> {
    let windows =
        [&app.main.window.handle, &app.commands.window.handle, &app.settings.window.handle, &app.reboot.window.handle];
    let events = windows
        .into_iter()
        .map(|handle| {
            let app = Rc::clone(app);
            nwg::full_bind_event_handler(handle, move |event, data, handle| app.on_event(event, &data, handle))
        })
        .collect();

    let raw_app = Rc::clone(app);
    let raw = nwg::bind_raw_event_handler(&app.main.window.handle, RAW_HANDLER_ID, move |_, msg, _, _| {
        raw_app.on_raw_message(msg)
    })?;
    Ok(Handlers { events, raw })
}
