# Implementation spec

Concrete, code-level plan for building Hollow Archive. Everything in §1 was read
directly from `zzz_packet_capture` (commit on `master` as of 2026-09-18, game
version 3.2 per its `assets/manifest.json`) and `irminsul` (`main`); nothing in
that section is inferred. Sections after it are design decisions for this project.

Read [overview.md](overview.md) and [architecture.md](architecture.md) first.
[testing.md](testing.md) is the companion testing plan.

---

## 1. Protocol facts (source of truth: `zzz_packet_capture`)

### 1.1 Licensing

- `zzz_packet_capture`: **MIT**. Porting logic to Rust is permitted; keep its
  copyright notice in `crates/hollow-proto/LICENSE-THIRD-PARTY` and credit it in
  the README.
- `irminsul`: **MIT / Apache-2.0** dual. Same treatment for any code lifted from
  `admin.rs` / `pktmon_backend.rs`.
- `auto-artifactarium`'s `cs_rand.rs` (C# `System.Random` port) is what
  `zzz_packet_capture` itself ported; we port it back to Rust from the Rust
  original. Attribute both.
- The RSA client private key and the Ec2b tables embedded in
  `zzz_packet_capture` are game-derived constants, not the author's copyrightable
  work. Ship them, but keep them in one file (`crypto/constants.rs`) so they're
  easy to replace when a client update rotates them.

### 1.2 Transport

| Item | Value |
|---|---|
| Transport | UDP, game server port **20501** (fixed; irminsul's 22101/22102 range is Genshin-only and does not apply) |
| Direction | `dst_port == 20501` → outgoing (client→server); otherwise incoming |
| Reliability layer | KCP, custom 28-byte header (stock KCP is 24; mihoyo adds a `token` field) |
| Capture backend (reference) | WinDivert on Windows, libpcap `any` device elsewhere |
| Capture backend (ours) | `pktmon` (default, no driver install), `pcap` fallback — see §3.2 |

### 1.3 KCP segment header (28 bytes, little-endian)

```
offset  size  field
0       4     conv
4       4     token
8       1     cmd        (81 = PUSH, 82 = ACK, 83 = WASK, 84 = WINS)
9       1     frg        (fragment countdown: last fragment has frg = 0)
10      2     wnd
12      4     ts
16      4     sn
20      4     una
24      4     len        (payload bytes following this header)
```

A UDP datagram holds one or more segments back-to-back. Only `cmd == PUSH` with
`len > 0` carries data. Reassembly (per direction, independent state):

1. First PUSH seen sets `rcv_nxt = sn`.
2. Segments with `sn >= rcv_nxt` (signed 32-bit wraparound compare) go into an
   ordered map keyed by `sn`; duplicates dropped.
3. Promote contiguous `sn`s from the map into a queue, incrementing `rcv_nxt`.
4. If the map exceeds 1024 entries, skip `rcv_nxt` forward to the lowest buffered
   `sn` (loss recovery — we're a passive observer and can't request retransmit).
5. From the queue head, a message is complete when `queue.len() >= frg + 1` and
   the run of fragments counts down `frg, frg-1, …, 0`. Concatenate payloads.
   If the run is malformed, pop the head and retry.

### 1.4 Game message envelope (inside a reassembled KCP message)

```
offset  size  field
0       4     magic (4 bytes; not validated by the reference — we will log mismatches)
4       2     cmd_id      BIG-endian
6       2     head_len    BIG-endian
8       4     body_len    BIG-endian
12      head_len   header protobuf (ignored)
12+head_len  body_len   body (XOR-encrypted protobuf)
```

Note the endianness split: KCP header is LE, message header is BE.

### 1.5 Encryption

Body encryption is a **4096-byte XOR pad**, applied as `body[i] ^= pad[i % 4096]`.

**Pad generation** (`xorpad::generate(seed: u64, big_endian: bool)`): seed
`std::mt19937_64`, draw 512 `u64`s, write each as 8 bytes — native LE for the
*initial* pad, byte-reversed (BE) for the *session* pad. Rust: the `rand_mt`
crate's `Mt19937GenRand64` (or a ~40-line hand port; MT19937-64 is well-specified
— verify against the C++ `std::mt19937_64` first output for seed 5489 =
the C++ standard's conformance value: the 10 000th output of a default-seeded
engine is `9981545732273789042`; that is what `crypto::mt19937_64` tests against).

**Initial pad seed** is per-region and per-game-version, shipped in
`datamine.json`:

```json
"xorSeeds": { "Europe": "9543521FC9C8CAED", "America": "50C21982AC009AF2",
              "Asia": "1A7E69FE2F49590A", "TW,HK,MO": "883B27204BF725D3" }
```

The reference can also derive these live from the dispatch server
(`query_dispatch` → per-region `dispatch_url` → RSA-decrypt `content` → JSON
`client_secret_key` → Ec2b blob → custom AES-unscramble + fold → u64 seed).
**v1 ships the table only**; Ec2b/dispatch derivation is a v1.1 item
(`crypto/ec2b.rs`, needs the ~4 KB of tables from `ec2b_tables.hpp`). The user
picks region in the UI, as the reference does.

**Session key handshake** (state machine in `hollow-proto::session`):

1. `Initial` — decrypt every incoming body with the initial pad.
2. On `cmd_id == cmdPlayerGetTokenScRsp` (**4937** in 3.2): parse the decrypted
   body as protobuf *unknown fields*. Find a length-delimited field whose
   base64-decoded value is exactly **128 bytes**. RSA-1024 decrypt it with the
   embedded client private key, **PKCS#1 v1.5 padding**. Result must be 8 bytes →
   `server_rand_key: u64` (LE). State → `HaveServerKey`.
3. Next incoming message with `body_len >= 32`: brute-force the client key from
   the packet's capture timestamp `t` (unix seconds):
   ```
   for delta in -5..=5:
     seed  = ((t + delta + 62_135_596_800) & 0xFFFF_FFFF) as i32   # .NET ticks-based seed
     rng   = CsRandom::new(seed)                                  # C# System.Random port
     hi    = rng.next_max(i32::MAX) as u32
     crk   = (hi as u64) << 32 | (seed as u32) as u64
     key   = server_rand_key ^ crk
     pad   = xorpad::generate(key, big_endian = true)
     if protobuf_unknown_fields_parse(body ^ pad).is_ok(): accept
   ```
   State → `Established { pad }`. From here all bodies use the session pad.
4. Outgoing traffic is never needed for export; we parse it only for KCP state.

`CsRandom` is a direct port of `auto-artifactarium/src/cs_rand.rs` (seed array of
56 `i32`, `MBIG = i32::MAX`, `MSEED = 161803398`, `inext=0, inextp=21`,
`next_max(n) = (sample as f64 * (1/MBIG)) * n`). Keep the arithmetic in `i32`
with explicit wrapping to match .NET.

### 1.6 Protobuf handling — no codegen

The reference does **not** compile `.proto` files. It parses bodies as
`google::protobuf::UnknownFieldSet` (wire-format only) and reads fields by number
using a small hand-maintained table (`datamine.json`). This is the right call for
us too: it means a game update needs a JSON bump, not a codegen run.

Two data files drive decoding:

**`datamine.json`** — command IDs and field numbers we care about (3.2 values):

```
cmdPlayerGetTokenScRsp 4937   cmdGetAvatarDataScRsp 2470
cmdGetEquipDataScRsp   3933   cmdGetWeaponDataScRsp 1382

agentData.agents = 2
agentInfo: id=2 level=3 promotion=14 weaponUid=12 mindscape=7 skills=4 dressed_equips=5
agentSkill: skill_type=1 level=9
agentEquip: uid=15 slot=10
equipData.discs = 5
discInfo: uid=14 id=6 level=1 mainStat=9 subStats=4
discStat: key=3 base_value=13 add_value=7
weaponData.weapons = 11
weaponInfo: id=13 uid=6 level=1 phase=12 modification=2
```

**`nap.json`** — full obfuscated proto schema dump (~MBs; message names are
obfuscated like `CKLPODCNKJB`). Entry shape:
`{name, cmd_id: u16|null, fields: [{number, name, type, repeated, is_native_type,
is_enum, xor_value: u32|null}]}`. Its only job for us is **per-field XOR
de-obfuscation**: after decrypting a body, look up the entry by `cmd_id`, and
for each scalar field with a non-zero `xor_value`, XOR the varint/fixed32/fixed64
value with it; recurse into nested message-typed fields by `type` name. Without
this, levels/IDs come out as garbage. Note the reference applies this to the top
level via `cmd_id` and to nested `data::*` structs via the datamine field numbers
*after* the XOR pass has already rewritten the nested bytes.

Rust: `protobuf` crate's `UnknownFields` is awkward for re-serialising nested
messages. Write a ~150-line wire-format reader/writer (`proto/wire.rs`: tag,
varint, fixed32/64, length-delimited) — it's simpler and lets us keep everything
as borrowed slices. No `prost`/`protobuf` dependency in `hollow-proto`.

### 1.7 Decoded data model (what the wire actually carries)

```rust
pub struct Agent      { id: u32, level: u32, promotion: u32, weapon_uid: u32,
                        mindscape: u32, skills: Vec<SkillLevel>, dressed: Vec<DressedEquip> }
pub struct SkillLevel { skill_type: u32, level: u32 }   // types on the wire: 0 basic, 1 special, 2 dodge, 3 chain, 5 core, 6 assist (4 never sent)
pub struct DressedEquip { uid: u32, slot: u32 }
pub struct WEngine    { id: u32, uid: u32, level: u32, phase: u32, modification: u32 }
pub struct DriveDisc  { uid: u32, id: u32, level: u32, main_stat: DiscStat, sub_stats: Vec<DiscStat> }
pub struct DiscStat   { key: u32, base_value: u32, add_value: u32 }  // add_value == roll count for substats
```

Three `bool` fields on discs and two on W-Engines are absent from upstream's
`datamine.json`; found by inspecting captures against `nap.json` types
(2026-09-19): disc **5** = lock, **7** = discard tag ("trash"), 10 = probably
"new/unviewed"; W-Engine **9** = lock, 4 = probably "viewed". Our vendored
`datamine.json` carries `discInfo.lock/trash` and `weaponInfo.lock` as extra
optional keys; upstream files without them decode with the flags off.

Drive disc `id` encodes everything: `rarity = id/10%10 + 1` (3=B,4=A,5=S),
`slot = id % 10` (1–6), `set = id/100*100`. Substats present on the wire are
exactly the activated ones; the reference pads the export to 4 entries with empty
keys (see §2 — we should *not* pad; verify Zenless Optimizer's accepted shape).

Stat key → ZOD stat name is a fixed 20-entry table (`statMap.hpp`), e.g.
`11102→hp_, 11103→hp, 12102→atk_, 20103→crit_, 21103→crit_dmg_, 23103→pen_,
23203→pen, 30502→enerRegen_, 31203→anomProf, 31402→anomMas_,
31503/31603/31703/31803/31903/32303 → physical_/fire_/ice_/electric_/ether_/wind_dmg_`.
Copy it verbatim into `export/stat_map.rs`.

### 1.8 Game data (ID → name)

The reference does **not** bundle names. It fetches from
**nanoka.cc**: `https://static.nanoka.cc/manifest.json` → `zzz.live` (version
string) → `https://static.nanoka.cc/zzz/{ver}/{character,equipment,weapon}.json`.
Lookups used: `characters[id].en` (+ `.rank`, 0-based rarity),
`equipment[set_id].en.name`, `weapons[id].en` (+ `.rank`). Cached to
`assets/nanokaData.json`, refreshed when the manifest version increases.

We do the same, with two changes: cache under the platform data dir
(`%LOCALAPPDATA%\hollow-archive\`), and **vendor a snapshot** in the repo so tests
and offline first-runs work (`crates/hollow-proto/data/nanoka-3.2.json`,
compressed with `flate2` at build time like irminsul does for `anime-game-data`).

### 1.9 Version tracking

The reference has `assets/manifest.json = {"version": "3.2"}` and checks GitHub
for a newer one at start-up ("New data files are available. Update?"). It does
**not** keep per-version proto snapshots — it breaks on every patch until the
maintainer re-dumps `datamine.json`/`nap.json` with GracefulDumper. We adopt:

- `hollow-proto` major version == ZZZ major.minor (e.g. `3.2.x`), same as
  `auto-artifactarium`.
- Data files (`datamine.json`, `nap.json`, `manifest.json`) are fetched from
  **`zzz_packet_capture`'s repo raw URLs** at runtime with a vendored fallback,
  exactly like their own updater does. We deliberately piggyback on their
  maintenance rather than running our own dumper for v1. This is the single
  biggest external dependency of the project; call it out in the README.

### 1.10 Zenless Optimizer export format ("ZOD")

Exact JSON, from `IZOD.hpp` and siblings:

```jsonc
{
  "format": "ZOD",
  "version": 1,
  "source": "Hollow Archive",
  "characters": [{                       // omitted entirely when export disabled
    "key": "ZhuYuan",                    // toZodKey(name): strip ' and -, non-alnum→space, UpperCamel, no spaces
    "level": 60, "mindscape": 0,
    "promotion": 5,                      // wire promotion - 1
    "core": 6,                           // skills[4].level - 1
    "basic": 12, "special": 12, "dodge": 12, "chain": 12, "assist": 12, // skills[0,1,2,3,5].level
    "potential": 0,                      // always 0
    "equippedEngine": "",                // always ""; ZO resets skills to 1 if the key is absent
    "id": "ZhuYuan"                      // == key
  }],
  "discs": [{
    "setKey": "WoodpeckerElectro",       // toZodKey(equipment[set].en.name)
    "slotKey": "4",                      // slot as string
    "level": 15,
    "rarity": "S",                       // B/A/S
    "mainStatKey": "crit_",
    "location": "ZhuYuan",               // agent key wearing it, or ""
    "lock": false, "trash": false,
    "substats": [{ "key": "atk_", "upgrades": 2 }, …]   // upgrades = add_value
  }],
  "wengines": [{
    "key": "StarlightEngine",
    "level": 60, "modification": 5, "phase": 5,
    "location": "ZhuYuan", "lock": false,
    "id": "zzz_wengine_123456"           // uid
  }]
}
```

Export filters in the reference: `minDiscRarity=3, minDiscLevel=0,
minEngineRarity=3, minEngineLevel=0, minAgentRarity=4, minAgentLevel=0`, plus
three include toggles. Agent/engine rarity comes from nanoka `rank + 1`.

---

## 2. Decisions and deviations from the reference

| # | Decision | Why |
|---|---|---|
| D1 | No `.proto` codegen; hand wire-format reader + JSON field tables | Matches reference; survives patches with a JSON bump. |
| D2 | Ship region seed table, defer Ec2b/dispatch derivation | Table covers all live regions; Ec2b needs tables + a live HTTPS round-trip to mihoyo on every start. |
| D3 | `pktmon` default backend, `pcap` behind a feature flag | Per architecture.md. pktmon yields **full L2 frames**; backend must strip Ethernet/IPv4/UDP headers itself (irminsul delegates this to auto-artifactarium — we do it in `capture/frame.rs`). |
| D4 | Fixture format = **pcapng** (primary), `captured_packets.json` (secondary import) | pcapng can be recorded today with Windows' built-in `pktmon` and inspected in Wireshark, so the whole decode pipeline is developed against one recording per game version — no relaunching the game per debugging session. See §3.4. The reference's JSON dump (`{packets:[{direction:0\|1, timestamp, data:[u8]}]}`) is also accepted for interop. |
| D5 | Do **not** pad `substats` to 4 entries with `""` keys | The reference does; it's almost certainly a bug ZO tolerates. Emit only real substats; confirm against ZO's importer in E2E (T-E2E-3). If ZO rejects, add padding behind a flag. |
| D6 | `source: "Hollow Archive"` | ZO shows source; don't impersonate the reference. |
| D7 | Export settings match the reference's names/defaults, not `data-model.md`'s draft | data-model.md's `min_agent_rank` (mindscape) filter doesn't exist in ZO's ecosystem; drop it. `include_materials` dropped: the reference doesn't capture materials, so there's no known cmd_id/field map. Materials are v2. |
| D8 | Update data files from `zzz_packet_capture`'s GitHub raw URLs | See §1.9. |
| D9 | Passive capture only — never inject or modify packets | pktmon is observe-only anyway; state it explicitly for ToS/anti-cheat clarity. |

`data-model.md` is superseded by §1.7 and §1.10; update it after this spec lands.

---

## 3. Crate design

### 3.1 `hollow-proto` (library, no UI or capture deps)

```
crates/hollow-proto/
├─ Cargo.toml                 # deps: rsa, base64, serde, serde_json, thiserror, rand_mt (or hand MT), tracing
├─ data/                      # vendored fallbacks: datamine.json, nap.json.gz, manifest.json, nanoka-<ver>.json.gz
└─ src/
   ├─ lib.rs
   ├─ error.rs                # ProtoError (thiserror)
   ├─ kcp.rs                  # Header::parse, Reassembler { incoming, outgoing }
   ├─ envelope.rs             # MessageHeader::parse, body slice
   ├─ crypto/
   │  ├─ constants.rs         # RSA key components (n, d as bytes), region seed table
   │  ├─ xorpad.rs            # generate(seed, big_endian) -> [u8; 4096]
   │  ├─ cs_random.rs         # CsRandom (C# System.Random)
   │  ├─ rsa.rs               # decrypt_pkcs1(block: &[u8;128]) -> Vec<u8>
   │  └─ session.rs           # Session state machine (§1.5)
   ├─ proto/
   │  ├─ wire.rs              # WireReader / WireWriter, Field { number, value: Varint|Fixed32|Fixed64|Bytes }
   │  ├─ schema.rs            # nap.json model + XorTable::apply(fields, cmd_id)
   │  └─ datamine.rs          # datamine.json model
   ├─ model.rs                # Agent, WEngine, DriveDisc, DiscStat, PlayerData
   ├─ decode.rs               # (cmd_id, fields) -> Option<Decoded { Agents | Discs | WEngines }>
   ├─ gamedata.rs             # nanoka types + lookup; NO networking (caller supplies bytes)
   ├─ pipeline.rs             # Pipeline::feed(udp_payload, direction, ts) -> Vec<Event>
   ├─ frame.rs                # strip Ethernet(+VLAN)/IPv4/IPv6/UDP -> (src_port, dst_port, payload); shared by fixtures and backends
   └─ fixture.rs              # pcapng reader/writer (`pcap-file` crate) + captured_packets.json import; yields (ts, dir, udp_payload)
```

Public API (what the app crate and the CLI use):

```rust
pub enum Direction { Incoming, Outgoing }

pub enum Event {
    ServerKeyFound,
    SessionEstablished,
    Agents(Vec<Agent>),
    WEngines(Vec<WEngine>),
    Discs(Vec<DriveDisc>),
    UnhandledCommand { cmd_id: u16, len: usize },
    Warning(String),          // non-fatal decode problems, surfaced in UI log
}

pub struct Pipeline { /* Reassembler, Session, Datamine, XorTable */ }
impl Pipeline {
    pub fn new(region_seed: u64, datamine: Datamine, schema: Schema) -> Self;
    pub fn feed(&mut self, udp_payload: &[u8], dir: Direction, unix_secs: i64) -> Vec<Event>;
    pub fn state(&self) -> SessionState;   // Initial | HaveServerKey | Established
}
```

`Pipeline` is fully deterministic given inputs — that's what makes fixture-driven
testing possible. Networking (fetching data files, nanoka) lives in the app crate.

### 3.2 `hollow-archive` (binary: app shell)

```
crates/hollow-archive/src/
├─ main.rs           # clap: --capture-backend {pktmon,pcap}, --region, --fixture <path>, --headless
├─ app.rs            # eframe::App; owns AppState, receives CaptureEvent over an mpsc channel
├─ state.rs          # AppState { phase, counts, player_data, log, settings, errors }
├─ admin.rs          # is_elevated() / relaunch_elevated()  — port of irminsul admin.rs
├─ capture.rs        # CaptureBackend trait, create_capture(); spawns tokio task: backend → frame → Pipeline → channel
├─ capture/
│  ├─ pktmon_backend.rs   # filter UDP port 20501; port of irminsul's, frames stripped via hollow_proto::frame
│  └─ pcap_backend.rs     # cfg(feature="pcap"); BPF "udp port 20501"
├─ datafiles.rs      # fetch/cache datamine.json, nap.json, manifest.json, nanoka; version compare; vendored fallback
├─ export.rs         # PlayerData + ExportSettings + GameData -> Zod (serde) ; to_zod_key()
├─ settings.rs       # ExportSettings + region + backend, persisted via eframe storage
└─ ui/
   ├─ theme.rs       # palette from ui.md (teal/near-black, amber accent)
   ├─ status.rs      # phase banner + counts
   ├─ export_panel.rs
   └─ log.rs
```

**Capture phase state machine** (drives the status banner from ui.md):

```
Idle ──start──▶ WaitingForGame (no packets on :20501 yet)
   ──first packet──▶ Handshake (Initial / HaveServerKey)
   ──SessionEstablished──▶ Capturing { agents, discs, wengines }
   ──all three seen──▶ Done (capture keeps running; user may stop)
Any ──error──▶ Error(kind)   kinds: NotElevated, BackendUnavailable, DataFilesMissing,
                                     HandshakeFailed(brute force window exhausted), CaptureClosed
```

Headless mode (`--headless`, the "first milestone" CLI from open-questions.md):
same pipeline, prints events as JSON lines to stdout, exits after `Done` or
timeout; `--fixture <file.pcapng|file.json>` replays a recording instead of
live capture; `--record <file.pcapng>` writes every frame seen on :20501 during
a live run (also exposed in the UI as "Save capture..." for bug reports).
Ship this as milestone M2 and keep it forever — it's the debugging tool.

### 3.4 Recording a fixture without any Hollow Archive code

Windows 10/11 ship `pktmon`. From an elevated shell:

```
pktmon filter remove
pktmon filter add zzz -t UDP -p 20501
pktmon start --capture --pkt-size 0 --file-name zzz-login.etl
#   launch ZZZ, log in, wait until the main menu is fully loaded
pktmon stop
pktmon pcapng zzz-login.etl -o zzz-login.pcapng
```

`--pkt-size 0` is required — the default truncates packets to 128 bytes.
Constraints on a recording:

- Timestamps are the decryption key input (§1.5 step 3). Replay must use the
  recorded per-packet timestamps; a fixture with rewritten timestamps is
  undecryptable.
- The initial XOR seed is per region **and per game version**, so a fixture is
  tied to the `datamine.json` of the version it was recorded on. Re-record once
  per patch. Name fixtures `<ver>-<region>-<desc>.pcapng`.
- Recordings contain the account's full inventory and the RSA-encrypted login
  token. Record on a throwaway account; `scripts/scrub-fixture` (M1) drops all
  outgoing payloads before a fixture is committed.

Development loop after one recording: `cargo test` (fixture replay + golden
export) and `hollow-archive --headless --fixture zzz-login.pcapng`. The game is
opened once per game version, plus the manual E2E gates in testing.md §5.

### 3.3 Dependencies (Rust)

| Crate | Use | Notes |
|---|---|---|
| `rsa` 0.9 | PKCS#1 v1.5 decrypt | Pure Rust; no OpenSSL. Construct from `(n, e, d)` via `RsaPrivateKey::from_components` (p,q optional → it'll be slower but we decrypt one block per session). |
| `base64` 0.22 | token field decode | |
| `rand_mt` | MT19937-64 | Or hand-port; must match `std::mt19937_64` exactly (test T-U3). |
| `serde`, `serde_json` | data files, export | |
| `thiserror`, `anyhow`, `tracing` | errors/logging | |
| `pktmon` 0.6 (windows) | capture | same as irminsul |
| (none) | pcapng read/write for fixtures and `--record` | `pcap-file` rejects options pktmon writes; `hollow_proto::pcapng` is a ~150-line SHB/IDB/EPB reader/writer instead |
| `pcap` 2 (feature) | fallback capture | |
| `eframe`/`egui` 0.32, `egui-notify`, `egui_extras` | UI | same versions as irminsul so the shell ports cleanly |
| `reqwest` (rustls) | data file + nanoka fetch | |
| `tokio` | capture task + fetches | |
| `clap` | CLI | |
| `directories` | cache dir | |
| `windows` (Shell) | elevation | via irminsul `admin.rs` |

`hollow-proto` must build on Linux/macOS (CI runs tests there); only the capture
backends are Windows-gated.

---

## 4. Milestones

| M | Deliverable | Exit criterion |
|---|---|---|
| M0 | Workspace scaffold, CI (fmt/clippy/test on ubuntu + windows), vendored data files, LICENSE attributions | `cargo test` green with placeholder tests |
| M0.5 ✅ | Record `3.2-<region>-fresh.pcapng` with `pktmon` (§3.4) before writing decode code | Done 2026-09-18: 20 797 datagrams, 0 drops, America |
| M1 ✅ | `hollow-proto`: kcp, envelope, xorpad, cs_random, rsa, session, wire, schema XOR, decode, pipeline, frame, pcapng/json fixture I/O | Done 2026-09-18: 46 unit + 5 replay tests; the recording decodes to 39/114/506 and the ZOD export is set-identical to the reference tool's export of the same account |
| M2 ✅ | `hollow-archive --fixture x.pcapng` and live capture via pktmon | Done 2026-09-18: live session on Win 11 went IDLE→DONE and its export is identical to the fixture replay |
| M3 ✅ (export) | `export.rs` + nanoka game data + settings — landed in `hollow-proto` rather than the app crate so the CLI and tests can use it | Golden test passes against the reference export; import into Zenless Optimizer still to be confirmed by hand (T-E2E-3) |
| M4 ✅ (mostly) | egui UI: status/phase banner, counts, export panel (copy/save), error surfaces, region picker, theme | Done 2026-09-18 except the data-file update prompt (moved to M5). Owner completed a live run without docs. |
| M5 ✅ (partial) | Data-file auto-update from upstream + nanoka (`datafiles.rs`, verified against live upstream 2026-09-18), persisted settings, log panel, save-capture toggle, `pcap` fallback behind a feature flag (compiles against Npcap SDK 1.15; not run-tested), irminsul-style frameless UI with hero art. Exe self-update via GitHub releases (`update.rs` + `release.yml`, added 2026-09-19). | Released as v3.2.0 |

Deferred (tracked, not scheduled): Ec2b live seed derivation; materials export;
running [GracefulDumper](https://github.com/AleXu224/GracefulDumper) ourselves
to regenerate `nap.json` (only if `zzz_packet_capture` goes unmaintained).

**Not an option: building on GracefulDumper directly.** It is an il2cpp DLL
injector that dumps C#/protobuf *definitions* from the game process at startup —
it is the tool upstream of `zzz_packet_capture` that produces `nap.json`, not a
packet capture. It sees no network traffic and no inventory, and injecting into
the game is exactly the anti-cheat exposure D9 rules out for an end-user tool.

---

## 5. Risks

1. **Upstream data-file dependency** (§1.9). Mitigation: vendored fallback,
   loud "data files are for 3.2, game is 3.3" banner, and `datamine.json` is small
   enough to maintain by hand if needed — `nap.json` (XOR table) is the hard one.
2. **pktmon frame format.** Resolved for Windows 11 (2026-09-18): the `pktmon`
   crate tags payloads (`Ethernet`/`IP`/`UDP`) and `capture.rs` strips each
   accordingly; the same packet arrives once per NDIS component, so the first
   component id seen is locked in. Windows 10 22H2 still untested.
3. **Timestamp-seeded brute force** depends on the *capture* timestamp being
   within ±5 s of the client's clock. pktmon timestamps are host-clock, same
   machine as the client → fine. Fixture replay must use the *recorded* timestamp,
   never `now()`.
4. **Fragmented/lossy login burst.** `GetEquipDataScRsp` for a large inventory is
   many KB across many KCP fragments; pktmon drops under load are possible.
   Mitigation: `Warning` events for gaps, and the UI's "Done" requires all three
   data messages, otherwise shows "partial capture — relaunch the game".
5. **Anti-cheat.** Passive capture only (D9). Same posture as irminsul/the
   reference; document that we never touch the game process.
