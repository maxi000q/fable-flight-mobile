# Bakes the Hornet's paint into assets/fa18_paint.jpg (plain Python, with Pillow and NumPy):
#
#   python assets/blender/fa18_paint.py [<dir of pictures>] [<out.jpg>]
#
# The paint is three pictures of the aircraft: from straight above, from
# straight below, and from its left side (assets/blender/paint/top.png,
# bottom.png, side.png). The game projects them back onto the skin
# (plane.wgsl, `hornet_paint`), so they must line up with the model: paint
# them over the guides that
#
#   blender -b --factory-startup --python assets/blender/fa18.py -- <scratch dir> --guides assets/blender/paint
#
# renders (<view>_guide.png, with the silhouette in <view>_mask.png). The ones
# here were painted over those guides by an image model (GPT Image, through
# the Codex CLI) and can be touched up or replaced in any paint program.
#
# This script cuts each picture to the model's silhouette, spreads the paint
# at its edge outward over the background (so the projection never picks up
# white at an edge) and lays the three side by side, the side view turned nose
# up like the others: top | bottom | side.
import os
import sys

import numpy as np
from PIL import Image, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "paint")
OUT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(HERE), "fa18_paint.jpg")


def spread(rgb, ok, rounds=128):
    """Grow the painted pixels outward, a pixel a round, over the ones that are not."""
    rgb = rgb.astype(np.float32) * ok[..., None]
    ok = ok.astype(np.float32)
    for _ in range(rounds):
        s = np.zeros_like(rgb)
        n = np.zeros_like(ok)
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                if dy or dx:
                    s += np.roll(rgb, (dy, dx), (0, 1))
                    n += np.roll(ok, (dy, dx), (0, 1))
        grow = (ok == 0) & (n > 0)
        if not grow.any():
            break
        rgb[grow] = s[grow] / n[grow][:, None]
        ok[grow] = 1
    rgb[ok == 0] = rgb[ok > 0].mean(0)
    return rgb


def layer(view):
    mask = Image.open(os.path.join(SRC, view + "_mask.png")).convert("L")
    paint = Image.open(os.path.join(SRC, view + ".png")).convert("RGB")
    if paint.size != mask.size:
        paint = paint.resize(mask.size, Image.LANCZOS)
    rgb = np.asarray(paint)
    # Painted: inside the silhouette (less a few pixels, where the picture's
    # own edge may fall short of the model's) and not the white of the background.
    inside = np.asarray(mask.filter(ImageFilter.MinFilter(7))) > 128
    ok = inside & (rgb.min(axis=2) < 236)
    out = Image.fromarray(np.clip(spread(rgb, ok), 0, 255).astype(np.uint8))
    print(f"{view}: {paint.size[0]}x{paint.size[1]}, {ok.mean() * 100:.0f}% painted")
    return out


top, bottom, side = layer("top"), layer("bottom"), layer("side")
side = side.transpose(Image.Transpose.ROTATE_270)  # nose up, like the other two
assert top.size == bottom.size == side.size, (top.size, bottom.size, side.size)
w, h = top.size
atlas = Image.new("RGB", (w * 3, h))
for k, im in enumerate((top, bottom, side)):
    atlas.paste(im, (w * k, 0))
atlas.save(OUT, quality=92, subsampling=0)
print("wrote", OUT, atlas.size, os.path.getsize(OUT), "bytes")
