# Third-party notices

Hollow Archive is a Rust port of, and depends on data produced by, the
following projects. Their licenses apply to the corresponding parts.

| Project | License | What we took |
|---|---|---|
| [AleXu224/zzz_packet_capture](https://github.com/AleXu224/zzz_packet_capture) | MIT | Protocol handling in `hollow-proto` (KCP framing, XOR pads, session key derivation, field tables, ZOD export shape); `datamine.json`, `nap.json`, `manifest.json` in `crates/hollow-proto/data/` |
| [AleXu224/GracefulDumper](https://github.com/AleXu224/GracefulDumper) (fork of thexeondev's) | see repo | Produces `nap.json` upstream; not bundled |
| [konkers/irminsul](https://github.com/konkers/irminsul) | MIT / Apache-2.0 | App structure, pktmon backend approach, UI layout |
| [IceDynamix/reliquary-archiver](https://github.com/IceDynamix/reliquary-archiver) | MIT | UAC relaunch (`admin.rs`, via irminsul) |
| [hashblen/auto-artifactarium](https://github.com/hashblen/auto-artifactarium) / [TheLostTree/evergreen](https://github.com/TheLostTree/evergreen) | MIT | .NET `System.Random` port (`cs_random.rs`) |
| [nanoka.cc](https://static.nanoka.cc) | — | Agent / W-Engine / Drive Disc set names (`nanoka.json`, fetched at runtime) |
| [Oswald](https://github.com/googlefonts/OswaldFont) | SIL OFL 1.1 | Display font (`crates/hollow-archive/assets/Oswald.ttf`, license alongside) |
| [Material Icons](https://github.com/google/material-design-icons) via `egui_material_icons` | Apache-2.0 | UI icons |

The hero artwork (`crates/hollow-archive/assets/hero.png`) is the Inter-Knot
post "A new Hollow on Fourteenth Street" from Zenless Zone Zero, © HoYoverse,
used here as a non-commercial fan-project backdrop. It will be removed on request.

Zenless Zone Zero is a trademark of HoYoverse / COGNOSPHERE. This project is
not affiliated with or endorsed by them.
