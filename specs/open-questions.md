# Open questions — answered

Resolved 2026-09-18 by reading `zzz_packet_capture` (master, game 3.2) and
`irminsul` (main). Details and citations are in [implementation.md](implementation.md).

1. **License compatibility** — `zzz_packet_capture` is MIT; `irminsul` is
   MIT/Apache-2.0. Porting is fine with attribution. → implementation.md §1.1
2. **Zenless Optimizer export schema** — exact "ZOD" v1 JSON captured from
   `IZOD.hpp` / `IAgent.cpp` / `IDisc.cpp` / `IEngine.cpp`, including the
   non-obvious bits (`promotion - 1`, `core - 1`, mandatory `equippedEngine: ""`,
   `id == key`, `zzz_wengine_<uid>`). → §1.10
3. **Protocol stability** — the reference has no per-version snapshots; it
   refetches `datamine.json` / `nap.json` from its own GitHub repo and breaks
   until re-dumped each patch. We version `hollow-proto` by game version and
   piggyback on their data files with a vendored fallback. → §1.9, risk 1
4. **pktmon on ZZZ** — port is a single fixed UDP **20501** (not irminsul's
   22101/22102). irminsul's pktmon backend ports over, but we must strip
   L2/L3/L4 headers ourselves. → §1.2, D3, risk 2, test T-I2
5. **Game data source of truth** — not bundled; fetched from nanoka.cc
   (`static.nanoka.cc/zzz/{ver}/{character,equipment,weapon}.json`). We vendor a
   snapshot. → §1.8

## Still open (tracked, not blocking)

- Does Zenless Optimizer accept `substats` with fewer than 4 entries? (D5,
  T-E2E-3.) The reference pads to 4 with empty keys.
- Is the pktmon `Packet.payload` a full Ethernet frame on every Windows build?
  (risk 2, T-I2.)
- Ec2b/dispatch live seed derivation — deferred to v1.1; region seed table
  covers all four live regions for 3.2.

## First milestone

Unchanged: headless CLI that replays a fixture or captures one login and dumps
decoded data as JSON lines (implementation.md M1–M2).
