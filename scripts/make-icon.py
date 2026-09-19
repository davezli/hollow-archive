"""Render the app icon (a Hollow: dark disc with a teal event horizon, amber signal)
into assets/icon.png (256px) and assets/icon.ico (16..256)."""
import math, pathlib
from PIL import Image, ImageDraw, ImageFilter

S = 1024  # supersample, then downscale
BG = (10, 18, 20); TEAL = (53, 196, 180); AMBER = (243, 179, 58); INK = (5, 9, 10)

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
r = S * 0.22
d.rounded_rectangle((0, 0, S - 1, S - 1), radius=r, fill=BG + (255,))

# faint teal haze behind the hollow
haze = Image.new("RGBA", (S, S), (0, 0, 0, 0))
hd = ImageDraw.Draw(haze)
c = (S * 0.5, S * 0.52); R = S * 0.30
hd.ellipse((c[0] - R * 1.35, c[1] - R * 1.35, c[0] + R * 1.35, c[1] + R * 1.35), fill=TEAL + (70,))
haze = haze.filter(ImageFilter.GaussianBlur(S * 0.08))
img.alpha_composite(haze)

d = ImageDraw.Draw(img)
# event horizon ring + dark disc
d.ellipse((c[0] - R - S * 0.03, c[1] - R - S * 0.03, c[0] + R + S * 0.03, c[1] + R + S * 0.03), fill=TEAL + (255,))
d.ellipse((c[0] - R, c[1] - R, c[0] + R, c[1] + R), fill=INK + (255,))
# ether debris: a few small teal shards
import random
random.seed(7)
for _ in range(14):
    a = random.uniform(0, 2 * math.pi); dist = random.uniform(R * 1.08, R * 1.45); sz = random.uniform(S * 0.012, S * 0.03)
    x, y = c[0] + math.cos(a) * dist, c[1] + math.sin(a) * dist
    d.polygon([(x, y - sz), (x + sz * 0.7, y + sz * 0.3), (x - sz * 0.5, y + sz * 0.5)], fill=TEAL + (200,))
# amber signal: a small dot low-right inside the disc, like neon signage under the Hollow
d.ellipse((c[0] + R * 0.35, c[1] + R * 0.45, c[0] + R * 0.35 + S * 0.075, c[1] + R * 0.45 + S * 0.075), fill=AMBER + (255,))

out = pathlib.Path(__file__).resolve().parent.parent / "crates/hollow-archive/assets"
png = img.resize((256, 256), Image.LANCZOS)
png.save(out / "icon.png")
png.save(out / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("wrote", out / "icon.png", out / "icon.ico")
