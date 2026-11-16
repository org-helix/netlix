netlix 0.3.0 installs like any other app: one file to download per system, and a welcome screen on first launch.

**Downloads**
| System | File |
|---|---|
| Windows 10 and 11 (x64) | `netlix-windows-setup.exe`: installer, no administrator rights needed |
| macOS 11 or later (Apple Silicon and Intel) | `netlix-macos.dmg`: open it and drag netlix to Applications |
| Linux (x86-64) | `netlix-linux.AppImage`: make it executable, then run it |

**New**
- **Welcome screen** on first launch: choose French or English and the dark or light theme. Both apply as you click. The language is suggested from your system's. **Start with an example** opens the office topology. Both settings stay under View.
- **Windows installer**: installs for your account without administrator rights, adds a Start menu entry and an optional desktop shortcut, opens `.netlix` files in netlix, and uninstalls from Settings → Apps. For a school or a whole machine: `/VERYSILENT` for a silent install, `/ALLUSERS` for every user (as administrator).
- **macOS**: a single app for Apple Silicon and Intel, in a disk image.
- **Linux**: a single AppImage that runs on most distributions (glibc 2.35 or later, e.g. Ubuntu 22.04+, Debian 12+, Fedora 36+).
- **App icon**: netlix has its own icon, in the window, the taskbar, the Dock and the installer.

The downloads are not code-signed yet. On Windows, SmartScreen may show "Windows protected your PC": choose More info → Run anyway. On macOS, the first launch is refused: open System Settings → Privacy & Security and choose Open Anyway.
