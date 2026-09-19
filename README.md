# Hollow Archive

Exports your Zenless Zone Zero Agents, W-Engines and Drive Discs to
[Zenless Optimizer](https://zzz.frzyc.com/) by watching the game's own login
traffic. No game files are touched and nothing is injected into the game.

Protocol handling is a Rust port of
[AleXu224/zzz_packet_capture](https://github.com/AleXu224/zzz_packet_capture) (MIT),
whose `datamine.json` / `nap.json` we also consume. App shell modelled on
[konkers/irminsul](https://github.com/konkers/irminsul).

## Use

1. Run `hollow-archive.exe` (it asks for administrator rights — packet capture needs them).
2. Pick your region, press **Start capture**.
3. Launch Zenless Zone Zero and log in. If the game is already open, log out to
   the title screen and back in.
4. When the status reads **DONE**, press **Copy to clipboard** and paste into
   Zenless Optimizer's import.

Windows 10/11 only. Uses the built-in `pktmon` driver, so no Npcap install.
Data files (packet schema, ID→name tables) update in-app when a new game version
ships upstream; the footer shows an **Update** button when one is available.

The hero art (`crates/hollow-archive/assets/hero.png`, the Inter-Knot "A new
Hollow on Fourteenth Street" post) is a ZZZ/Wikia asset and is not committed;
without it the app draws a plain backdrop. Confirm licensing before shipping it.

## Develop

Everything below the capture driver is deterministic and runs offline against a
recording, so you only need the game once per game version:

```
# record once (elevated shell)
pktmon filter add zzz -t UDP -p 20501
pktmon start --capture --pkt-size 0 --file-name zzz-login.etl
#   ... launch the game, log in, reach the main menu ...
pktmon stop
pktmon pcapng zzz-login.etl -o zzz-login.pcapng

# then, forever after
cargo run -p hollow-archive -- --headless --region America --fixture zzz-login.pcapng -o out.json
cargo run -p hollow-archive -- --fixture zzz-login.pcapng          # same, in the GUI
cargo test --workspace                                              # drop the pcapng in crates/hollow-proto/tests/fixtures/ to enable replay tests
```

### Npcap fallback

`cargo build -p hollow-archive --features pcap` adds an Npcap backend (`--capture-backend pcap`,
or the Backend row in capture settings). Building it needs the
[Npcap SDK](https://npcap.com/#download): set `LIB=<sdk>\Libd` first. Running it needs Npcap installed.

See `specs/` for the design, protocol notes and test plan.
