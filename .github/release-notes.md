netlix 0.2.0 looks inside the ping: every Ethernet frame is simulated and explained, the app speaks French by default, and the window is laid out again with more room for the canvas.

**Frame-level simulation**
- Each interface has a MAC address. Hosts and routers keep an **ARP cache**, so a missing entry sends a broadcast ARP request and the packet waits for the reply. Switches learn a **MAC address table**, forward known destinations and flood the rest. A loop between switches is stopped and reported as a broadcast storm.
- Caches and tables are kept between pings, so the second ping skips address resolution. Clear one device by right-clicking it, or all of them from the console or the Edit menu.

**Two send modes**
- *Real time* plays the whole exchange. With **Explain every frame** on, the log lists every frame with the reason it was sent and its simplified headers (Ethernet, ARP, IPv4, ICMP).
- *Step by step* stops after each step (Next / Back / Play, or the arrow keys). You see the frames on their cables, why each was sent, what each device did when it arrived, and the caches as they were at that step.

**Languages**
- French by default, English under View → Language. Everything is translated, including the explanations of every frame.

**Window**
- New title bar in the app's colours, with the menus, the document name and the window buttons.
- The right-hand panel is gone. Right-click a device → **Configure…** (or double-click it) to edit it in a floating window. Right-click a cable to delete it.
- The duplicate-IP warning that appeared while typing an address has been removed.

**Downloads**
| Platform | File |
|---|---|
| Windows (x64) | `netlix-*-x86_64-pc-windows-msvc.zip` |
| Linux (x64) | `netlix-*-x86_64-unknown-linux-gnu.tar.gz` |
| macOS (Apple Silicon) | `netlix-*-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `netlix-*-x86_64-apple-darwin.tar.gz` |

The binaries are not code-signed. On Windows, SmartScreen may ask you to confirm ("More info → Run anyway"). On macOS, right-click the binary and choose Open the first time, or run `xattr -d com.apple.quarantine netlix`.
