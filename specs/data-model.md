# Data model

ZZZ's equivalents of Genshin's exportable data, and how they map onto irminsul's
`good.rs` concepts. Field names below are placeholders pending confirmation against
`zzz_packet_capture`'s actual parsed structs — do not treat these as final until
checked against that source.

| Genshin (irminsul) | ZZZ (Hollow Archive) | Notes |
|---|---|---|
| Character | Agent | Level, Rank (Mindscape Cinema, ZZZ's constellation equivalent), Core Skill level, combat/skill levels |
| Weapon | W-Engine | Level, Refinement ("Modification" rank), Ascension |
| Artifact (5 slots) | Drive Disc (6 slots) | Main stat, substats + roll counts, including uninitialized/unactivated substat handling like irminsul's `fake_uninitialized_4th_line` |
| Material | Material | Inventory materials, by ID + count |
| GOOD format | Zenless Optimizer format | Exact schema TBD — pull from `zzz_packet_capture`'s export code, do not invent this from scratch |

## Export settings

Port irminsul's `ExportSettings` shape directly, adjusted for ZZZ's stat ranges:

```
include_agents: bool
include_drive_discs: bool
include_w_engines: bool
include_materials: bool

min_agent_level: u32
min_agent_rank: u32          # Mindscape Cinema count

min_drive_disc_level: u32
min_drive_disc_rarity: u32

min_w_engine_level: u32
min_w_engine_modification: u32
min_w_engine_rarity: u32
```

## Game data source

irminsul pulls Genshin ID -> name/skill-type mappings from a separate
`anime_game_data` crate. ZZZ needs the equivalent: a mapping of Agent IDs, W-Engine
IDs, and Drive Disc set IDs to human-readable names. Check whether `zzz_packet_capture`
already ships this data (it must, to produce its export) before building a new one —
this is exactly the kind of already-solved problem from `overview.md`'s "why build
this" list. Source ZZZ game data from `zzz_packet_capture`'s repo/data files, not by
hand-transcribing from the wiki.
