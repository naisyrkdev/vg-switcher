# AGENTS.md

Notes for AI agents working on VG Switcher (Rust, Windows-only, `native-windows-gui` + custom dark mode in `src/darkmode.rs`).

## UI rules

### Labels: height must match the text (no white "lanes")

`nwg::Label` vertically centres its text by shrinking the client area and painting the leftover space above and below as non-client area with `COLOR_WINDOW`, which is **white**. Our dark theme (`WM_CTLCOLORSTATIC` in `darkmode.rs`) only colours the client area, so any label taller than its text shows white bands above and below it.

- Make every label exactly `LABEL_H` tall, holding **one line** of text.
- For multi-line text, use **one label per line**, stacked with `LABEL_H + LABEL_GAP` spacing. Never put `\n` in one tall label.
- Don't "fix" it with `.background_color(...)` on the builder. That colour is fixed when the label is built and won't follow a theme switch (Dark/Light/Dracula).

### New windows

Build every new window like `build_settings_ui` / `build_reboot_ui`:

1. Create it without `VISIBLE`, then call `darkmode::apply_titlebar_theme(hwnd)` and `darkmode::install_dark_background(hwnd)`.
2. Style every button with `darkmode::make_filled_button` or `darkmode::make_text_button` (with a `Role`), never the native look.
3. Bind its events with `nwg::full_bind_event_handler`, handle `OnWindowClose` with `hide_on_close`, and unbind it at the end of `main`.
4. Store it on `AppState` and show it with `set_visible(true)` + `set_focus()`.

Native message boxes (`nwg::modal_message`, `show_info`) are fine for simple OK / Yes-No questions. When the buttons need custom text, build a window.

### Colours

Never hard-code RGB values. Use the palette functions (`window_bg()`, `text_fg()`, `success()`, `error()`, …) or a `Role`, so theme switching keeps working.

## Building and running

- The exe asks for admin rights (manifest in `build.rs`). A running copy locks `target/release/VG_Switcher.exe`, so `cargo build --release` fails until it's closed.
- A normal shell can't kill the elevated process. Use `Start-Process taskkill -ArgumentList '/F /IM VG_Switcher.exe' -Verb RunAs -Wait` (shows a UAC prompt).
- Read through a new window after building it: screenshot it, or ask the user to check it, in every theme if you touched colours.

## Release

- Version lives in `Cargo.toml`, `installer/VG_Switcher.iss` (fallback only) and the README badge. Keep them in sync and run `cargo update -w` so `Cargo.lock` matches (CI builds with `--locked`).
- Pushing a `v*` tag runs `.github/workflows/release.yml`, which publishes the installer and the portable zip.
