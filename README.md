# netlix

A small network simulator in the spirit of Packet Tracer. Draw a topology, set IP addresses, and ping between devices. It is a single native binary for Windows, Linux and macOS, written in Rust with [egui](https://github.com/emilk/egui).

![netlix in dark mode: two LANs behind a router with an Internet uplink, router settings on the right, ping results below](docs/screenshot.png)

## Download

Prebuilt binaries for Windows, Linux and macOS (Intel and Apple Silicon) are on the [Releases](https://github.com/org-helix/netlix/releases) page. Unzip and run `netlix`; nothing to install.

## Features

- **Canvas**: place PCs, servers, switches, routers and an Internet cloud. Drag devices to move them, scroll to zoom, drag the background to pan.
- **Links**: the Link tool (`L`) connects two devices on their first free ports. Port names are shown at each end of the link.
- **Configuration**: IP address and subnet mask for each interface (`255.255.255.0`, `/24` or `24`), a default gateway on hosts, and static routes on routers. Invalid values are flagged as you type. Duplicate IPs, network and broadcast addresses, and off-subnet gateways are reported as warnings.
- **Ping**: an ICMP echo is simulated hop by hop and animated on the canvas. The request is drawn in red and the reply in the text colour. The console explains every failure, for example a missing gateway, no route, an unreachable host, a lost reply or an expired TTL.
- **Files**: New, Open, Save and Save as. Topologies are stored as readable, versioned JSON `.netlix` files. A file path given on the command line is opened at startup. If you try to close with unsaved changes, netlix asks first.
- **Themes**: dark by default, light under **View → Dark mode**. The choice is remembered between sessions.

## Simulation model

| Device | Behaviour |
|---|---|
| PC, Server | One NIC (`eth0`). Delivers on-link when the destination is in its subnet, otherwise sends to the default gateway. Never forwards traffic. |
| Switch | 8 ports with no IP. Floods frames to every linked port (loop-safe). |
| Router | 4 routed ports. Uses the longest-prefix match among connected networks and static routes, and decrements the TTL. |
| Internet | Same as a router with 4 `wan` ports. Give it a `/32` (e.g. `8.8.8.8`) to stand for a public host. |

A ping only succeeds when both the request and the reply get through.

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

Right-click a device for "Ping from here", "Link from here" and "Delete".

## Development

```sh
cargo test     # simulation, file format and model tests
cargo clippy --all-targets
```

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed.

On Linux, building needs `libxkbcommon-dev libwayland-dev libgl1-mesa-dev` and the `libxcb-*` headers (see `.github/workflows/ci.yml`). File dialogs use the XDG portal, so GTK is not needed.

Code map: `model.rs` (topology data), `sim.rs` (ping), `io.rs` (file format), `canvas.rs` (drawing and interaction), `panels.rs` (toolbox, inspector, console), `theme.rs` (Helix look), `app.rs` (state, menus, files).

## Design

netlix uses the Helix colour palette on top of the Dusk DS structure: Bricolage Grotesque titles, Geist text, a 4 px grid, 8/12/16 radii, and no drop shadows. The dark theme is the dark Helix variant: a deepened ink canvas (`#1E1914`), Pale Amber text, Red `#B24E30` as the only action colour and Blush for focus. The light theme puts ink on Pale Amber (`#FAF6ED`). All colours live in `src/theme.rs`, and a unit test checks WCAG AA contrast for both themes. Fonts are bundled under the SIL Open Font License (`assets/fonts/OFL-*.txt`).
