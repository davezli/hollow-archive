# Hollow Archive — Overview

## What it is

A packet-capture data exporter for Zenless Zone Zero (ZZZ), in the spirit of
[irminsul](https://github.com/konkers/irminsul) (which does this for Genshin Impact,
exporting to the GOOD format for Genshin Optimizer).

Hollow Archive captures the game's own network traffic during the login handshake,
decodes the Agent / W-Engine / Drive Disc data mihoyo's servers send the client, and
exports it in a format consumable by Zenless Optimizer (or whatever the community's
`GOOD`-equivalent for ZZZ ends up being).

Name: **Hollow Archive** is a ZZZ faction dedicated to researching Hollows and
cataloguing Ether data — appropriate for a tool whose entire job is archiving data
pulled out of the game.

## Why build this instead of using what exists

[AleXu224/zzz_packet_capture](https://github.com/AleXu224/zzz_packet_capture) already
does this, in C++, and already exports to Zenless Optimizer. It is the reference
implementation for the hard part of this project (packet identification, decryption,
protobuf schema). Reasons to still build Hollow Archive:

- Its UI is not good; this project's main motivation is a better UI/UX, built in
  Rust with `egui` (same toolkit irminsul uses).
- Rust over C++ for memory safety and to match the rest of this tool ecosystem
  (irminsul, `auto-artifactarium`/`reliquary`) so patterns and possibly code can be
  shared or cross-referenced later.

## Precedent this is modeled on

The Genshin/HSR scanner ecosystem already did a version of this port once:
[hashblen/auto-artifactarium](https://github.com/hashblen/auto-artifactarium) (the
crate irminsul depends on for Genshin packet parsing) is itself a fork/adaptation of
[IceDynamix/reliquary](https://github.com/IceDynamix/reliquary), which was originally
written for Honkai: Star Rail. So "take the packet-capture + protobuf-decode approach
from one mihoyo/HoYoverse game and retarget it at another" has already worked once
in this exact tool family. We are doing the same move again, this time Genshin → ZZZ,
using `zzz_packet_capture` as the source of protocol truth instead of doing the
reverse-engineering from scratch.

## Goals

- Reliable, fast capture of Agent, W-Engine, and Drive Disc data (including
  "unactivated"/uninitialized substat rolls, matching irminsul's artifact behavior).
- Export to Zenless Optimizer's expected JSON schema.
- A UI that doesn't suck: clear status of capture state, live progress as data comes
  in, sane defaults, minimal clicks from "open app" to "data copied to clipboard."
- Single Windows binary (matching ZZZ's primary platform), admin/elevation handled
  the way irminsul does it (`pktmon` on Windows to avoid requiring an Npcap install).

## Non-goals (for v1)

- Linux/macOS support (ZZZ has no official client there; revisit only if demand shows up).
- Wish/signal (gacha) history export.
- Real-time updates while the game is running (matches irminsul's own "planned, not done" status).
- Writing our own protobuf reverse-engineering from raw traffic — we lean on
  `zzz_packet_capture`'s already-solved decryption/key handling and message layout.

## Reference projects (read before implementing)

| Project | Role |
|---|---|
| [konkers/irminsul](https://github.com/konkers/irminsul) | UI shell (egui) and app structure to imitate/improve on. Genshin-specific logic (GOOD export, `anime_game_data`) is NOT reusable. |
| [AleXu224/zzz_packet_capture](https://github.com/AleXu224/zzz_packet_capture) | Source of truth for ZZZ's packet framing, encryption/key handling, and message IDs. This is the part that must be ported into Rust. |
| [hashblen/auto-artifactarium](https://github.com/hashblen/auto-artifactarium) | Example of what a Rust protobuf-decode crate for a HoYoverse capture tool looks like structurally. |
| [IceDynamix/reliquary](https://github.com/IceDynamix/reliquary) | The original HSR version of the above; useful for seeing how the codegen/versioning problem (game updates break proto schemas) was handled long-term. |

See [architecture.md](architecture.md), [data-model.md](data-model.md), and
[open-questions.md](open-questions.md) for the rest of the spec.
