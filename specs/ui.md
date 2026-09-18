# UI

The stated reason for doing this at all instead of just using `zzz_packet_capture`.
"The UI sucks on the other one" is the bar to clear — be specific about what "sucks"
means before redesigning blind; check `zzz_packet_capture`'s actual UI first and note
concretely what's wrong with it (missing progress feedback? confusing capture state?
cluttered layout? no dark mode?) rather than assuming.

## Requirements carried over from irminsul (these already work well there)

- Simple, clean single-window layout — no need to reinvent this shape, egui + a
  narrow fixed-ish window (irminsul ships ~420x640-ish) suits a utility like this.
- Export settings as visible filters (checkboxes + min-level/min-rarity fields),
  not a hidden config file.
- Two export actions: copy to clipboard, save to file.

## Things to actually improve

- **Capture state clarity**: at all times the user should be able to tell, at a
  glance: not capturing / waiting for game handshake / capturing / done, and what's
  been captured so far (counts of Agents/W-Engines/Drive Discs seen). This is the
  single most common complaint pattern in this genre of tool ("is it doing anything?").
- **Errors surfaced in-UI**, not just in logs — e.g. "couldn't find Npcap", "capture
  needs admin", "game not detected" should show as actionable UI messages.
- **First-run flow**: a new user shouldn't need to read a README to get through
  elevation prompt -> launch game -> see data appear. Confirm this path with a
  Windows walkthrough before calling v1 done (see `run` skill for a way to drive
  the built app and check this by hand rather than assuming).

## Visual design reference

User-supplied reference image: ["A new Hollow on Fourteenth Street"](https://static.wikia.nocookie.net/zenless-zone-zero/images/c/c5/Inter-Knot_Post_A_new_Hollow_on_Fourteenth_Street.png) —
an Inter-Knot in-universe news post depicting a Hollow opening over a New Eridu
street. Use this as the app's mood/theme anchor:

- **Palette**: dark teal/cyan-on-near-black, matching the image's night-city grade —
  not a generic egui light/dark default. Accent color pulled from the image's warm
  neon signage (amber/yellow) for primary actions (capture button, export button)
  against the cool background, so calls-to-action pop against the moody backdrop.
- **Mood**: quiet, ominous, liminal — an Inter-Knot field report, not a cheerful
  gacha-tracker vibe. Fits "Hollow Archive" as a name (in-world, this is what an
  Inter-Knot researcher's tool would look like).
- **Application**: use as a splash/header background (faded/darkened behind the
  capture-status panel) rather than tiling it everywhere — it's a scene-setting
  image, not a texture. A cropped/blurred strip behind the title bar is enough;
  don't let it fight with the data tables for attention.
- Respect image licensing: this is a Wikia/community asset tied to ZZZ, not
  redistributable game-original art — treat it as a local dev reference for palette
  and mood, confirm licensing before shipping it as bundled app assets.

## Explicitly deferred

- Animations, mascot art (Bangboo iconography could be a fun v2 touch, not a blocker).
