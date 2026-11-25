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

## Build

```sh
cargo build --release
```

The binary lands in `target/release/VG_Switcher.exe`. It requires administrator rights to run.

To build the installer, compile `installer/VG_Switcher.iss` with [Inno Setup](https://jrsoftware.org/isinfo.php).

## Note

Changes to Vanguard's services take effect after a reboot.

## License

[MIT](LICENSE)
