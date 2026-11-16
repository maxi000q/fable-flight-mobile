# Packs NASA's Black Marble 2016 grayscale night lights (public domain, credit
# NASA Earth Observatory) into assets/night_lights.bin for sources.rs `NightLights`.
#   https://assets.science.nasa.gov/content/dam/science/esd/eo/images/imagerecords/144000/144897/BlackMarble_2016_{A1,B1,C1,D1,A2,B2,C2,D2}_gray.jpg
# (eight 21600x21600 tiles, 15 arc seconds per pixel: A..D = 90 degree columns
# from 180 W, 1 = north half, 2 = south half).
# Usage: python3 -I mknight.py DIR_WITH_THE_EIGHT_JPEGS assets/night_lights.bin
#
# Format (little endian): b"NLM2", u16 block side B (px), u8 level count, then
# per level u16 columns, u16 rows, u16 degrees per block; then u32 offsets for
# all levels' blocks in turn (level by level, row by row from 90 N, columns from
# 180 W) plus one at the end, into the data that follows. Block i is
# data[off[i]:off[i + 1]]: zlib, B*B bytes row by row (north first); empty = dark.
# Level 0 is the full 15", each next one 6x coarser (averaged in linear light).
import struct, sys, zlib
import numpy as np
from PIL import Image

Image.MAX_IMAGE_PIXELS = None
src, out = sys.argv[1], sys.argv[2]
B = 240          # pixels across a block (one degree at level 0)
FLOOR = 12       # below this it is JPEG noise and faint airglow, not lights
STEP = 4         # levels kept: finer steps are JPEG noise and only cost space
DOWN = 6         # each level is this much coarser than the one before
GAMMA = 2.2

def quant(a):
    return np.where(a < FLOOR, 0, (a // STEP) * STEP + STEP // 2).astype(np.uint8)

def pack(a, deg):
    rows, cols = a.shape[0] // B, a.shape[1] // B
    out = []
    for r in range(rows):
        for c in range(cols):
            blk = a[r * B:(r + 1) * B, c * B:(c + 1) * B]
            out.append(zlib.compress(np.ascontiguousarray(blk).tobytes(), 9) if blk.max() else b'')
    return (cols, rows, deg, out)

# Level 0 straight from the tiles; level 1 (40 px/deg) gathered as they go.
l0 = [[b''] * 360 for _ in range(180)]
l1 = np.zeros((180 * B // DOWN, 360 * B // DOWN), np.float32)
for ti, t in enumerate('ABCD'):
    for half in (1, 2):
        a = quant(np.asarray(Image.open(f'{src}/{t}{half}.jpg').convert('L')))
        for by in range(90):
            for bx in range(90):
                blk = a[by * B:(by + 1) * B, bx * B:(bx + 1) * B]
                if blk.max():
                    l0[(half - 1) * 90 + by][ti * 90 + bx] = zlib.compress(np.ascontiguousarray(blk).tobytes(), 9)
        lin = (a.astype(np.float32) / 255.0) ** GAMMA
        n = a.shape[0] // DOWN
        y0, x0 = (half - 1) * n, ti * n
        l1[y0:y0 + n, x0:x0 + n] = lin.reshape(n, DOWN, n, DOWN).mean(axis=(1, 3))
        print(t, half, flush=True)

def enc(lin):
    return quant(np.clip(np.round(lin ** (1 / GAMMA) * 255.0), 0, 255).astype(np.uint8))

h, w = l1.shape
l2 = l1.reshape(h // DOWN, DOWN, w // DOWN, DOWN).mean(axis=(1, 3))
levels = [(360, 180, 1, [b for row in l0 for b in row]), pack(enc(l1), 6), pack(enc(l2), 36)]

offs, o = [], 0
for lv in levels:
    for b in lv[3]:
        offs.append(o)
        o += len(b)
offs.append(o)
with open(out, 'wb') as f:
    f.write(b'NLM2' + struct.pack('<HB', B, len(levels)))
    for cols, rows, deg, _ in levels:
        f.write(struct.pack('<HHH', cols, rows, deg))
    f.write(struct.pack(f'<{len(offs)}I', *offs))
    for lv in levels:
        for b in lv[3]:
            f.write(b)
for i, lv in enumerate(levels):
    print('level', i, sum(1 for b in lv[3] if b), 'blocks with light,', sum(len(b) for b in lv[3]), 'bytes')
