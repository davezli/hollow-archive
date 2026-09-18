# Architecture

Mirrors irminsul's module split, since that structure works and the goal is a better
UI on the same skeleton, not a redesign of the app's shape.

```
hollow-archive/
├─ crates/
│  ├─ hollow-archive/          # binary: app shell + UI (egui)
│  │  ├─ app.rs                # top-level egui App impl, view state
│  │  ├─ admin.rs               # elevation prompt (packet capture needs it on Windows)
│  │  ├─ capture.rs             # capture session lifecycle, wires backend -> protocol -> app state
│  │  ├─ export.rs              # ZZZ player data -> Zenless Optimizer JSON
│  │  ├─ update.rs              # self-update check (optional for v1)
│  │  └─ capture/
│  │     └─ pcap_backend.rs     # libpcap/Npcap backend (portable fallback)
│  │     └─ pktmon_backend.rs   # Windows pktmon backend (default; no driver install needed)
│  │
│  └─ hollow-proto/             # library: ZZZ protocol decode, ported from zzz_packet_capture
│     ├─ packet.rs              # framing: find/parse the game's packet envelope
│     ├─ crypto.rs              # key exchange / decryption, ported from zzz_packet_capture
│     ├─ proto/                 # protobuf message defs for the messages we care about
│     └─ messages.rs            # typed structs for Agent/W-Engine/Drive Disc payloads
│
└─ specs/                       # this directory
```

## Why split into two crates

`hollow-proto` is the part that breaks every time ZZZ ships a client update (message
IDs and proto schemas shift), same as `reliquary`/`auto-artifactarium` version their
proto codegen separately from the app that consumes it (see overview.md's versioning
note from `auto-artifactarium`'s README: they bump major version per game version).
Keeping it a separate crate means:

- It can be versioned/pinned independently of the UI.
- It's testable against captured packet fixtures without spinning up the UI.
- If someone else wants just the decode logic (e.g. for a CLI or a different UI),
  they can depend on `hollow-proto` alone.

## Capture pipeline (matches irminsul's flow, ZZZ payloads instead of Genshin's)

1. **Backend** (`pktmon_backend` / `pcap_backend`) captures raw packets to/from the
   game process during the login handshake.
2. **`hollow-proto`** finds the handshake key exchange, decrypts subsequent traffic,
   and decodes the specific message types that carry Agent/W-Engine/Drive Disc state.
3. **`capture.rs`** in the app crate owns the capture session, feeds decoded messages
   into in-memory player-data state, and pushes UI updates (progress, item counts).
4. **`export.rs`** takes the accumulated player-data state + user export settings
   (filters like irminsul's `ExportSettings`: min level, min rarity, etc.) and
   serializes to the Zenless Optimizer JSON schema.
5. UI: clipboard copy or save-to-file, same as irminsul.

## Platform target

Windows only for v1, `pktmon` as default backend (no driver installation required —
this was explicitly called out as a UX win in irminsul's README and is worth keeping).
`pcap`/Npcap as a documented fallback via `--capture-backend pcap`, mirroring
irminsul's CLI flag.
