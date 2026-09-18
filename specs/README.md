# Hollow Archive — specs

Read in this order:

1. [overview.md](overview.md) — what this is, why, and what it's modeled on
2. [architecture.md](architecture.md) — module/crate split
3. [data-model.md](data-model.md) — ZZZ data model draft (superseded in part by implementation.md §1.7/§1.10)
4. [ui.md](ui.md) — UI requirements and visual design reference
5. [open-questions.md](open-questions.md) — unknowns, now answered, with pointers into implementation.md
6. [implementation.md](implementation.md) — protocol facts read from `zzz_packet_capture`, crate design, milestones, risks
7. [testing.md](testing.md) — test layers, test IDs, fixtures, CI, definition of done per milestone

No code has been written yet. Next step is milestone M0 in `implementation.md`
(workspace scaffold + CI + vendored data files), then M1 (`hollow-proto`).
