<p align="center"><img src="assets/logo.png" alt="VG Switcher logo" width="160"></p>

# VG Switcher

![Version](https://img.shields.io/badge/version-0.5.0-blue)
![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust&logoColor=white)
![Platform](https://img.shields.io/badge/platform-Windows-0078D6?logo=windows&logoColor=white)
![License](https://img.shields.io/badge/license-MIT-green)
![Inno Setup](https://img.shields.io/badge/installer-Inno%20Setup-264B99)

A small Windows tray app for turning Riot Vanguard on and off without digging through services.

## Features

- Enable / disable Vanguard with one click (from the window or the tray menu)
- Shows the current Vanguard status
- Kill the lingering Vanguard tray process (`vgtray.exe`)
- Editable enable/disable commands
- Optional autostart with Windows
- Light, Dark and Dracula themes

## Download

Grab the installer or the portable `.zip` from [Releases](https://github.com/naisyrkdev/vg-switcher/releases).

## Build

```sh
cargo build --release
```

The binary lands in `target/release/VG_Switcher.exe`. It requires administrator rights to run.

Run the tests with `cargo test`.

To build the installer, compile `installer/VG_Switcher.iss` with [Inno Setup](https://jrsoftware.org/isinfo.php).

## Notes

- Changes to Vanguard's services take effect after a reboot.
- Settings and the log are stored in `%ProgramData%\VG_Switcher`, writable only by administrators (the app runs elevated and executes the configured commands). Settings from older versions in `%APPDATA%\VG_Switcher` are moved there automatically.
- For autostart, prefer the installer: the autostart task runs the exe with administrator rights, so it should live in Program Files rather than a user-writable folder.

## License

[MIT](LICENSE)
