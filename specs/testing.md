# Testing plan

Companion to [implementation.md](implementation.md). Test IDs (`T-U3`, `T-E2E-2`)
are referenced from there. The guiding constraint: **the decode pipeline is pure
and deterministic** (`Pipeline::feed(bytes, dir, ts) -> Vec<Event>`), so
everything below the capture backend is testable on any OS without the game.

## 1. Test layers

| Layer | Runs where | Needs game? | Tooling |
|---|---|---|---|
| Unit (`hollow-proto`) | CI: ubuntu + windows | No | `cargo test` |
| Fixture replay (`hollow-proto`, `export`) | CI | No — uses recorded `captured_packets.json` | `cargo test`, `insta` for golden JSON |
| Property / fuzz | CI (short), nightly (long) | No | `proptest`; `cargo-fuzz` targets for `kcp`, `wire`, `envelope` |
| Backend integration | Windows dev box, manual + optional CI self-hosted runner | Partially (needs any UDP traffic on :20501; a local echo script suffices for the frame path) | `--headless` |
| End-to-end | Windows dev box, manual | Yes | `--headless` then UI |
| UI walkthrough | Windows dev box, manual | Yes | checklist §5 |

## 2. Unit tests (`hollow-proto`)

### 2.1 `kcp`

- **T-U1** `Header::parse` on a hand-built 28-byte LE buffer → every field
  round-trips; `< 28` bytes → `Err`, not panic.
- **T-U2** Reassembler:
  - single PUSH, frg=0 → one message.
  - 3 fragments in order (frg 2,1,0) → one concatenated message.
  - same 3 fragments arriving 0,2,1 across three `feed` calls → one message,
    emitted on the third call.
  - duplicate `sn` → ignored.
  - sn gap then map overflow (>1024) → `rcv_nxt` skips forward, later messages
    still decode (loss recovery).
  - `sn` wraparound near `u32::MAX` → signed-diff logic keeps ordering.
  - ACK/WASK/WINS segments mixed in → ignored; PUSH with `len == 0` → ignored.
  - a datagram whose declared `len` exceeds remaining bytes → stop parsing that
    datagram, no panic, earlier segments in it are kept.
  - incoming and outgoing streams do not share state.

### 2.2 `envelope`

- **T-U3a** BE parse of `cmd_id/head_len/body_len`; body slice starts at
  `12 + head_len` and is exactly `body_len` long; truncated body → `Err`.

### 2.3 `crypto`

- **T-U3** `xorpad::generate`: MT19937-64 conformance — seed `5489`, first
  output `0x8fb7dbd0b0d7ebd0` … (standard reference vector; assert the first 3).
  Then assert `generate(seed, false)[0..8] == first_u64.to_le_bytes()` and
  `generate(seed, true)[0..8] == first_u64.to_be_bytes()`. Pad length 4096.
- **T-U4** `CsRandom`: port the test vectors from `auto-artifactarium`'s
  `cs_rand.rs` tests verbatim (seed → first N `next()` values, plus
  `next_max(i32::MAX)`). Also seed `i32::MIN` (the `abs` special case) must not
  overflow.
- **T-U5** `seed_from_unix_secs(1_700_000_000)` == the value the reference
  computes: `(1_700_000_000 + 62_135_596_800) & 0xFFFF_FFFF` as `i32`. Assert
  the truncation-to-i32 semantics explicitly (it can be negative).
- **T-U6** `rsa::decrypt_pkcs1`: generate a known plaintext, encrypt it with the
  *public* half `(n, e=65537)` in the test using the `rsa` crate, decrypt with
  our private key → identical. Wrong-size input → `Err`. Bad padding → `Err`
  (no panic).
- **T-U7** `Session` state machine with synthetic messages (no real capture):
  1. Build a fake `PlayerGetTokenScRsp` body: protobuf with one length-delimited
     field = base64(RSA-encrypt(server_rand_key LE bytes)) plus decoy fields
     (a short bytes field, a varint). XOR it with the initial pad for a test
     seed. Feed → `ServerKeyFound`, `server_rand_key` matches.
  2. Pick `ts`, compute expected `crk` from `CsRandom`, build a valid protobuf
     body, XOR with `generate(server ^ crk, true)`. Feed with `ts + 3` (inside
     the ±5 window) → `SessionEstablished`. Feed with `ts + 9` → stays
     `HaveServerKey`, emits `Warning`.
  3. Body `< 32` bytes after `HaveServerKey` → not used for brute force.
  4. After `Established`, a body that fails protobuf parse → `Warning`, state
     unchanged (never regress to `Initial`).

### 2.4 `proto`

- **T-U8** `wire`: round-trip every wire type; varint edge cases (0, 127, 128,
  `u64::MAX`); malformed (truncated varint, length-delimited overrun, wire type
  6/7) → `Err`; writer output re-parses identically (needed by XOR pass on
  nested messages).
- **T-U9** `schema` XOR: load a minimal `nap.json` snippet with a top-level
  entry `cmd_id=42` having a varint field `xor_value=5927` and a nested
  message-typed field whose type has its own XOR field. Encode, apply, assert
  both levels de-obfuscated; fields without `xor_value` untouched; unknown
  `cmd_id` → no-op.
- **T-U10** `datamine`: the vendored `datamine.json` deserialises and every
  field number is non-zero (guards against a half-edited update).

### 2.5 `decode` / `model`

- **T-U11** `DriveDisc` id decomposition: `id = 31543` → set 31500, rarity 5
  (S), slot 3; `id = 31221` → rarity 3 (B), slot 1. Table-driven over all 6 slots
  × 3 rarities.
- **T-U12** Build a `GetEquipDataScRsp`-shaped field set using datamine numbers
  (repeated field 5, nested disc with uid/id/level/mainStat/subStats) → decodes to
  the expected `Vec<DriveDisc>`, substat count equals what was encoded (2, 3, 4).
- **T-U13** Same for agents (skills vector length 6, `dressed_equips`) and
  W-Engines.
- **T-U14** Unknown `cmd_id` → `Event::UnhandledCommand`, never `Err`.

### 2.6 `gamedata`

- **T-U15** Vendored nanoka snapshot parses; lookups for a few pinned IDs return
  the expected `en` names and `rank`s; missing ID → `None` (never panics —
  a new agent after a patch must degrade to `"Unknown_<id>"` in export, not crash).

## 3. Fixture replay & golden tests

Fixtures live in `crates/hollow-proto/tests/fixtures/`. Format is **pcapng**
recorded with `pktmon` (implementation.md §3.4); the reference tool's
`captured_packets.json` is also accepted (D4). **Fixtures contain a real account's
inventory and the session's RSA-encrypted token** — record with a throwaway
account, and before committing run `scripts/scrub-fixture` which replaces the
outgoing stream with empty bodies (we only need incoming) and confirms no
plaintext UID/nickname appears. Keep each fixture < 2 MB.

Required fixtures (recorded once per supported game version, named
`<ver>-<region>-<desc>.json`):

| Fixture | Purpose |
|---|---|
| `3.2-eu-fresh.pcapng` | Small inventory, clean login. Baseline. **Record this first (M0.5)** — every other test in this section derives from it. |
| `3.2-eu-large.pcapng` | 500+ discs; exercises multi-fragment reassembly. |
| `3.2-eu-reordered` | `fresh` with incoming datagrams shuffled within a 4-packet window (generated in-test, not recorded). |
| `3.2-eu-lossy` | `fresh` with 2 % of incoming datagrams dropped (generated in-test). Expected: `Warning`s, and either full decode or a clearly reported partial. |
| `3.2-us-fresh.pcapng` | Second region seed. |
| `3.2-eu-fresh.json` | `fresh` converted to the reference's JSON format by our writer, for the interop tests. |

- **T-F1** Replay each fixture through `Pipeline` with the recorded timestamps
  → asserts: `ServerKeyFound` then `SessionEstablished` occur exactly once and
  in that order; counts of agents/discs/wengines equal the numbers stored in the
  fixture's sidecar `<name>.expect.json` (written when recording, from what the
  reference tool printed).
- **T-F2** Replay with every timestamp shifted +3 s → still establishes (window
  tolerance). Shifted +30 s → `HandshakeFailed` warning, no data events. This
  pins the risk-3 behaviour.
- **T-F3** Golden export: `PlayerData` from `3.2-eu-fresh` + default
  `ExportSettings` + vendored nanoka → `insta` snapshot of the ZOD JSON. Review
  the snapshot by hand once against §1.10 of implementation.md (key casing,
  `promotion - 1`, `core - 1`, `equippedEngine: ""`, `id == key`,
  `zzz_wengine_<uid>`, `slotKey` is a string).
- **T-F4** Export filters: each setting individually (min level, min rarity,
  include toggles) changes the snapshot in the expected direction; disabling a
  category omits the key entirely (not `[]`).
- **T-F5** `to_zod_key` table: `"Zhu Yuan"→"ZhuYuan"`, `"Soldier 11"→"Soldier11"`,
  `"Nekomiya Mana"→"NekomiyaMana"`, `"Anby Demara"→"AnbyDemara"`, `"Von Lycaon"→"VonLycaon"`,
  `"[Magnetic Storm] Alpha"→"MagneticStormAlpha"`, `"Ben Bigger"→"BenBigger"`,
  hyphen/apostrophe stripping `"Sharpshooter's Gaze"→"SharpshootersGaze"`.
  Cross-check the character list against Zenless Optimizer's key list once
  (manual, T-E2E-3) and pin any exceptions here.
- **T-F6** Fixture round-trip: pcapng load → save → load yields identical
  (ts, dir, payload) triples; pcapng → `captured_packets.json` → replay gives the
  same event sequence as the pcapng (so recordings from either tool are
  interchangeable).
- **T-F7** Frame stripping on the real recording: every packet in
  `3.2-eu-fresh.pcapng` parses as Ethernet→IPv4→UDP with one port == 20501;
  a packet with an 802.1Q tag and an IPv6 packet (hand-built) also parse.
  This is the CI-side half of T-I2.

## 4. Property & fuzz

- **T-P1** `proptest`: random KCP segmentations of a random message (random
  fragment sizes, random order within a window, random duplicate injection)
  always reassemble to the original.
- **T-P2** `proptest`: random `Vec<Field>` → `WireWriter` → `WireReader` identity.
- **T-P3** `cargo-fuzz` targets: `fuzz_kcp_feed`, `fuzz_wire_parse`,
  `fuzz_envelope`, `fuzz_pipeline_feed` (pipeline seeded with the vendored data
  files). Invariant: no panics, no allocations > 64 MB per call (guards
  against trusting `body_len`/`len` from the wire). Run 10 min per target in
  nightly CI; 30 s in PR CI.

## 5. Windows integration & end-to-end (manual, gate for M2–M4)

Environment matrix: Windows 11 (current), Windows 10 22H2. Record results in
`docs/test-log.md` per release.

### 5.1 Backend (`T-I*`)

- **T-I1** Not elevated → app shows the "needs administrator" error and offers
  relaunch; `--headless` exits non-zero with the same message.
- **T-I2** `frame.rs` on real pktmon output: log the first 64 bytes of the first
  captured frame once; confirm Ethernet→IPv4→UDP parse and that `dst_port` is
  20501 for outgoing. (Risk 2 in implementation.md.) Also send a UDP datagram to
  `127.0.0.1:20501` with a Python one-liner and confirm it arrives through the
  backend — validates filter + frame strip without the game.
- **T-I3** `--capture-backend pcap` build with Npcap installed behaves
  identically for T-I2; without Npcap → actionable error naming Npcap.
- **T-I4** Stop/start capture 5× in one process → no leaked pktmon session
  (check `pktmon list` after exit shows nothing of ours).

### 5.2 Live capture (`T-E2E*`)

- **T-E2E-1** `--headless` running, launch ZZZ, log in. Expect, in order:
  `WaitingForGame → Handshake → ServerKeyFound → SessionEstablished → Agents,
  WEngines, Discs` within the loading screen. Record the run to a fixture
  (`--record out.json`) and compare counts with the in-game inventory.
- **T-E2E-2** Launch the app *after* the game is already at the main menu →
  UI stays in `WaitingForGame` with the hint "log out and back in / restart the
  game". Then relog → data arrives.
- **T-E2E-3** Export → paste into Zenless Optimizer's importer. Pass criteria:
  no import errors; agent count/levels/skills match; discs land on the right
  agents (`location`); a disc with 3 substats shows 3, not a blank 4th (D5);
  W-Engine phase/modification correct. Try each region seed with an account on
  that region if available.
- **T-E2E-4** Wrong region selected → handshake never establishes;
  `HandshakeFailed` banner suggests checking region. (Confirms failure is
  diagnosable, not silent.)
- **T-E2E-5** Data-file update prompt: temporarily edit cached
  `manifest.json` to `3.1` → prompt appears; accept → files refetched; decline →
  app runs with old files and shows a persistent "data files may be outdated"
  notice.
- **T-E2E-6** Offline start (disable network): app starts from vendored data,
  no crash, warning about nanoka refresh; capture still works (it doesn't need
  network).

### 5.3 UI first-run walkthrough (M4 exit criterion)

Fresh Windows user profile, no README. A tester who has never seen the app must
be able to: open the exe → accept elevation → pick region → press Start → launch
the game → see the banner move through the phases and the three counts fill →
press Copy → paste into ZO. Time it; target < 3 minutes excluding game load.
Any step where the tester asks "what do I do now?" is a defect. Also check:

- Status text is legible at 100 % and 150 % DPI scaling.
- Amber primary button is the only strongly-coloured element on screen at rest.
- Errors (T-I1, T-I3, T-E2E-4) appear in-window, not just in the log panel.
- Window is resizable and content reflows to the ~420×640 default.

## 6. CI

```
on: [push, pull_request]
jobs:
  test-linux:   ubuntu-latest  — fmt, clippy -D warnings, cargo test --workspace (hollow-archive with --no-default-features; UI compiles but nothing runs)
  test-windows: windows-latest — same, plus `cargo build --release` of hollow-archive and a smoke `hollow-archive --headless --fixture tests/fixtures/3.2-eu-fresh.json`
  fuzz-short:   ubuntu-latest  — 30 s per fuzz target
  nightly:      schedule       — 10 min per fuzz target; fixture replay with all fixtures
```

`hollow-proto` tests must not touch the network. Anything that fetches
(`datafiles.rs`, nanoka refresh) is tested with `wiremock` in the app crate.

## 7. Definition of done per milestone

| M | Required green |
|---|---|
| M0.5 | `3.2-eu-fresh.pcapng` recorded per implementation.md §3.4 and opens in Wireshark |
| M1 | §2 all, T-F1 and T-F7 on `3.2-eu-fresh.pcapng`, T-P1/P2, fuzz-short |
| M2 | + T-I1, T-I2, T-E2E-1 |
| M3 | + T-F3–F6, T-E2E-3 |
| M4 | + §5.3 walkthrough, T-E2E-2/4/5/6 |
| M5 | + T-I3, T-I4, nightly job green for a week |
