"""Fetch nanoka.cc ZZZ game data and write the slim vendored snapshot used by hollow-proto.

Usage: python scripts/update-gamedata.py [version]   (default: manifest's zzz.live)
Output: crates/hollow-proto/data/nanoka.json  -> {version, characters{id:{en,rank}}, equipment{set_id:{en}}, weapons{id:{en,rank}}}
"""
import json, sys, urllib.request, pathlib

def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "hollow-archive/update-gamedata"})
    with urllib.request.urlopen(req) as r:
        return json.loads(r.read().decode("utf-8"))

ver = sys.argv[1] if len(sys.argv) > 1 else get("https://static.nanoka.cc/manifest.json")["zzz"]["live"]
base = f"https://static.nanoka.cc/zzz/{ver}"
chars = get(f"{base}/character.json")
equip = get(f"{base}/equipment.json")
weap = get(f"{base}/weapon.json")
out = {
    "version": ver,
    "characters": {k: {"en": v["en"], "rank": v["rank"]} for k, v in chars.items()},
    "equipment": {k: {"en": v["en"]["name"]} for k, v in equip.items()},
    "weapons": {k: {"en": v["en"], "rank": v["rank"]} for k, v in weap.items()},
}
dst = pathlib.Path(__file__).resolve().parent.parent / "crates/hollow-proto/data/nanoka.json"
dst.write_text(json.dumps(out, indent=1, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8")
print(f"wrote {dst} (v{ver}: {len(chars)} chars, {len(equip)} sets, {len(weap)} weapons)")
