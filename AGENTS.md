# AGENTS.md

Notes for AI agents working on VG Switcher (Rust, Windows-only, `native-windows-gui` with custom theming in `src/theme/`).

## Layout

The app is a library (`src/lib.rs`) plus a one-line binary (`src/main.rs`), so the logic can be unit-tested.

| Module | Role |
|---|---|
| `platform/` | Safe wrappers over Win32: services (SCM), processes (ToolHelp), registry, ACLs, known folders, single instance, `schtasks`/`shutdown`. All `unsafe` non-UI code lives here. |
| `config.rs` | `Config` + `ConfigStore` (atomic save, admin-only folder, migration from the old `%APPDATA%` location). |
| `vanguard.rs`, `riot.rs` | Domain logic. No UI. |
| `theme/` | `mod.rs` palette model, `win32.rs` title bars and background subclass, `controls.rs` owner-drawn buttons and toggles. |
| `ui/` | One file per window plus `tray.rs`. `widgets.rs` holds the shared building blocks. Windows map clicks to an `Action` and don't act on their own. |
| `action.rs`, `app.rs` | `Action` enum and `App::dispatch`, the only place actions are carried out. |

To add a feature: add an `Action` variant, map a button or menu item to it in the window's `action_for`, and handle it in `App::dispatch`.

## UI rules

### Labels: height must match the text (no white "lanes")

`nwg::Label` vertically centres its text by shrinking the client area and painting the leftover space above and below as non-client area with `COLOR_WINDOW`, which is **white**. Our theme (`WM_CTLCOLORSTATIC` in `theme/win32.rs`) only colours the client area, so any label taller than its text shows white bands above and below it.

- Make every label exactly `ui::LABEL_H` tall, holding **one line** of text.
- For multi-line text, use **one label per line**, stacked with `LABEL_H + LABEL_GAP` spacing. Never put `\n` in one tall label.
- Don't "fix" it with `.background_color(...)` on the builder. That colour is fixed when the label is built and won't follow a theme switch (Dark/Light/Dracula).

### New windows

Build every new window like `ui/settings_window.rs` / `ui/reboot_window.rs`:

1. Create it with `widgets::themed_window(...)`. It's created hidden and already has the themed title bar and background.
2. Build buttons with `widgets::filled_button` / `widgets::text_button` (with a `Role`), never the native look. Toggles use `widgets::Toggle`, status readouts use `widgets::StatusRow`.
3. Position controls with `Placeable::place`, `widgets::place_row` and `place_row_right` instead of hand-computed x offsets.
4. Give it an `action_for(handle) -> Option<Action>`, add it to `App`, to `App::action_for_button`, to the `OnWindowClose` hide list in `App::on_event`, and to `bind_events`.
5. Show it with `ui::present(&window)`.

Use `ui::dialog::{error, info, confirm}` for message boxes, with the window the action came from as owner. When the buttons need custom text, build a window instead.

### Colours

Never hard-code RGB values. Use the palette functions (`theme::window_bg()`, `text_fg()`, `success()`, `error()`, …), a `Role`, or `ui::status_view::StatusView`, so theme switching keeps working.

## Security rules

The app runs elevated (manifest in `build.rs`) and runs command lines from its config.

- The config and log live in `%ProgramData%\VG_Switcher`, made admin-only by `platform::security`. Don't move them anywhere a standard user can write.
- Never build a shell command line from config or user input. Use the Win32 wrappers in `platform/`. External tools are started by absolute path (`paths::system_tool`), never by bare name.
- Don't parse localized console output. Use APIs or exit codes.

## Building and running

- `cargo test` runs the unit tests (library only; the GUI binary isn't started). CI (`.github/workflows/ci.yml`) runs `fmt --check`, `clippy -D warnings`, tests and a release build on every push and PR.
- The exe asks for admin rights. A running copy locks `target/release/VG_Switcher.exe`, so `cargo build --release` fails until it's closed.
- A normal shell can't kill the elevated process. Use `Start-Process taskkill -ArgumentList '/F /IM VG_Switcher.exe' -Verb RunAs -Wait` (shows a UAC prompt).
- Only one instance runs at a time. A second launch brings the first one's window forward and exits.
- Runtime log: `%ProgramData%\VG_Switcher\vg_switcher.log`.
- Read through a new window after building it: screenshot it, or ask the user to check it, in every theme if you touched colours.

## Release

- The version lives in `Cargo.toml` and the README badge. Keep them in sync and run `cargo update -w` so `Cargo.lock` matches (CI builds with `--locked`). The installer reads the version from the built exe.
- Pushing a `v*` tag runs `.github/workflows/release.yml`, which publishes the installer and the portable zip.
