First release of netlix, a small native network simulator: draw a topology, set IP addresses, ping.

**Features**
- Canvas with PCs, servers, switches, routers and an Internet cloud. Drag devices, scroll to zoom, link with the Link tool.
- IP address and mask for each interface, a default gateway on hosts, and static routes on routers, all validated as you type.
- Animated ping with an explanation for every failure: no gateway, no route, host unreachable, lost reply, or TTL expired.
- New, Open, Save and Save as for `.netlix` files. An example topology is included (`office.netlix`, also under File → Load example).
- Dark theme by default. Switch to light under View → Dark mode; the choice is remembered.

**Downloads**
| Platform | File |
|---|---|
| Windows (x64) | `netlix-*-x86_64-pc-windows-msvc.zip` |
| Linux (x64) | `netlix-*-x86_64-unknown-linux-gnu.tar.gz` |
| macOS (Apple Silicon) | `netlix-*-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `netlix-*-x86_64-apple-darwin.tar.gz` |

The binaries are not code-signed. On Windows, SmartScreen may ask you to confirm ("More info → Run anyway"). On macOS, right-click the binary and choose Open the first time, or run `xattr -d com.apple.quarantine netlix`.
