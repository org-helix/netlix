# netlix

A small network simulator in the spirit of Packet Tracer. Draw a topology, set IP addresses, and ping between devices. It is a single native binary for Windows, Linux and macOS, written in Rust with [egui](https://github.com/emilk/egui).

![netlix in dark mode: two LANs behind a router with an Internet uplink, router settings on the right, ping results below](docs/screenshot.png)

## Download

Prebuilt binaries for Windows, Linux and macOS (Intel and Apple Silicon) are on the [Releases](https://github.com/org-helix/netlix/releases) page. Unzip and run `netlix`; nothing to install.

## Features

- **Canvas**: place PCs, servers, switches, routers and an Internet cloud. Drag devices to move them, scroll to zoom, drag the background to pan.
- **Links**: the Link tool (`L`) connects two devices on their first free ports. Port names are shown at each end of the link.
- **Configuration**: right-click a device → **Configure…** (or double-click it) to open its settings in a floating window, which leaves the whole width to the canvas. The window holds the IP address and subnet mask for each interface (`255.255.255.0`, `/24` or `24`), a default gateway on hosts, and static routes on routers. Invalid values are flagged as you type, as are network and broadcast addresses and off-subnet gateways. Each interface shows its MAC address.
- **Ping, frame by frame**: every Ethernet frame is simulated (ARP requests and replies, switch forwarding and flooding, routed ICMP echo request and reply) and animated on its cable. ARP frames are hollow rings, echo requests red, echo replies in the text colour. The console explains every failure, for example a missing gateway, no route, no ARP reply, a lost reply, an expired TTL or a broadcast storm.
- **Two send modes**:
  - *Real time* plays the whole exchange at once. With **Explain every frame** on, the log lists each frame with the reason it was sent and its simplified headers (Ethernet, ARP, IPv4, ICMP).
  - *Step by step* stops after every step. Use **Next / Back / Play / First / Last** or the arrow keys to see the frames on the wire, why each was sent, and what each device did on receiving it. Click a frame to highlight it on the canvas. The inspector shows ARP caches and MAC tables as they were at that step.
- **ARP caches and MAC tables**: hosts and routers keep an ARP cache and switches learn a MAC address table. Both survive between pings, so the second ping skips address resolution. They are shown in each device's configuration window. Clear one device by right-clicking it (**Clear ARP cache** / **Clear MAC table**), or all of them from the console or **Edit**.
- **Files**: New, Open, Save and Save as. Topologies are stored as readable, versioned JSON `.netlix` files. A file path given on the command line is opened at startup. If you try to close with unsaved changes, netlix asks first.
- **Title bar**: netlix draws its own title bar instead of the system one. It holds the menus, the document name (with a dot when unsaved) and minimise / maximise / close. Drag it to move the window, double-click to maximise, and resize from any edge. On macOS the native window buttons are kept.
- **Themes**: dark by default, light under **View → Dark mode**. The choice is remembered between sessions.
- **Languages**: French (default) and English, under **View → Language** (**Affichage → Langue**). Everything is translated: menus, panels, error messages and the explanation of every frame. Switching language re-runs the ping being stepped through so its explanations follow. New devices are named in the current language (Routeur1 / Router1).

## Simulation model

| Device | Behaviour |
|---|---|
| PC, Server | One NIC (`eth0`). Delivers on-link when the destination is in its subnet, otherwise sends to the default gateway. Never forwards traffic. |
| Switch | 8 ports with no IP. Learns source MACs per port, forwards known unicast, floods broadcasts and unknown destinations. There is no spanning tree, so a switching loop causes a broadcast storm; the simulation stops it and says so. |
| Router | 4 routed ports. Uses the longest-prefix match among connected networks and static routes, resolves the next hop with ARP, and decrements the TTL. |
| Internet | Same as a router with 4 `wan` ports. Give it a `/32` (e.g. `8.8.8.8`) to stand for a public host. |

A ping only succeeds when both the request and the reply get through. MAC addresses are `02:4e:` followed by the device number and the interface index, so they stay the same when a file is reopened.

## Usage

```sh
cargo run --release                           # empty canvas
cargo run --release -- examples/office.netlix # two LANs + Internet uplink
```

You can also use **File → Load example** for the same topology.

| Shortcut | Action |
|---|---|
| `1`–`5` | Place PC / Server / Switch / Router / Internet |
| `L`, `V`, `Esc` | Link tool, select tool, cancel |
| `Del` | Delete selection |
| `Ctrl+N/O/S`, `Ctrl+Shift+S` | New, open, save, save as |
| `Ctrl+0` | Reset view |
| `→` or `Space`, `←` | Next or previous step (step-by-step mode) |

Right-click a device for "Configure…", "Ping from here", "Link from here", "Clear ARP cache" (or "Clear MAC table" on a switch) and "Delete". Right-click a cable to delete it. `Esc` closes the configuration window.

## Development

```sh
cargo test     # simulation, file format and model tests
cargo clippy --all-targets
```

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed.

On Linux, building needs `libxkbcommon-dev libwayland-dev libgl1-mesa-dev` and the `libxcb-*` headers (see `.github/workflows/ci.yml`). File dialogs use the XDG portal, so GTK is not needed.

Code map: `i18n.rs` (languages: every text is written in French and English where it is used, with `tr(fr, en)` or `tf!(fr, en)`), `model.rs` (topology data), `sim.rs` (frame-level ping, ARP, switching, routing), `io.rs` (file format), `canvas.rs` (drawing and interaction), `panels.rs` (toolbox, inspector, console), `theme.rs` (Helix look), `titlebar.rs` (custom title bar, window move and resize), `app.rs` (state, menus, files).

## Design

netlix uses the Helix colour palette on top of the Dusk DS structure: Bricolage Grotesque titles, Geist text, a 4 px grid, 8/12/16 radii, and no drop shadows. The dark theme is the dark Helix variant: a deepened ink canvas (`#1E1914`), Pale Amber text, Red `#B24E30` as the only action colour and Blush for focus. The light theme puts ink on Pale Amber (`#FAF6ED`). All colours live in `src/theme.rs`, and a unit test checks WCAG AA contrast for both themes. Fonts are bundled under the SIL Open Font License (`assets/fonts/OFL-*.txt`).
