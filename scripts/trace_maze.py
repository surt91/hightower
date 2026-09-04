#!/usr/bin/env python3
"""Vectorises Hightower's Hampton Court maze plot (paper p. 19).

Input: the 300 dpi bilevel scan of page 19
(`pdfimages -f 19 -l 19 -png references/hightower.pdf /tmp/hc`).
The thin lines are the maze walls Hightower fed his program, the thick line is
the path his program found. The script separates the two by stroke width,
classifies every ink pixel as part of a horizontal or a vertical stroke by the
length of the run through it, groups the pixels into segments and scales the
result to integer units (1 unit = SCALE pixels). Coordinates that lie within a
few pixels of each other are merged first, so walls that meet in the scan also
meet in the output.
"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

SCALE = float(sys.argv[3]) if len(sys.argv) > 3 else 7.0  # pixels per unit; corridors are ~22 px, so ~3 units wide

src = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/hc-000.png")
out = Path(sys.argv[2] if len(sys.argv) > 2 else "examples/data/hampton_court.txt")

img = np.array(Image.open(src).convert("L")) < 128  # True = ink

# crop to the frame (largest connected component)
labels, n = ndimage.label(img)
sizes = ndimage.sum(img, labels, range(1, n + 1))
ys, xs = np.where(labels == 1 + int(np.argmax(sizes)))
y0, y1, x0, x1 = ys.min(), ys.max(), xs.min(), xs.max()
pad = 6
crop = img[y0 - pad : y1 + pad + 1, x0 - pad : x1 + pad + 1]
print(f"frame {x1 - x0} x {y1 - y0} px", file=sys.stderr)

# thick strokes (the path) survive an erosion that kills the thin walls
thick = ndimage.binary_dilation(ndimage.binary_erosion(crop, np.ones((15, 15))), np.ones((15, 15)))
thin = crop & ~thick
# the labels "A" and "B" and small specks: drop thin components with a tiny bounding box
lab, n = ndimage.label(thin)
for i, sl in enumerate(ndimage.find_objects(lab), start=1):
    h, w = sl[0].stop - sl[0].start, sl[1].stop - sl[1].start
    if max(h, w) < 20:
        thin[lab == i] = False


def run_lengths(mask, axis):
    """For every pixel, the length of the True run through it along `axis`."""
    m = mask if axis == 1 else mask.T
    out_ = np.zeros(m.shape, dtype=np.int32)
    for r in range(m.shape[0]):
        row = m[r]
        d = np.diff(np.concatenate(([0], row.astype(np.int8), [0])))
        for s, e in zip(np.where(d == 1)[0], np.where(d == -1)[0]):
            out_[r, s:e] = e - s
    return out_ if axis == 1 else out_.T


def strokes(mask, max_width, min_len):
    """Axis-parallel strokes: pixels whose run perpendicular to the stroke is
    short belong to a stroke of that orientation. Returns
    (horizontal [(y, x_from, x_to)], vertical [(x, y_from, y_to)]) in pixels."""
    vrun = run_lengths(mask, 0)  # vertical run through each pixel
    hrun = run_lengths(mask, 1)
    horiz = mask & (vrun <= max_width) & (hrun > vrun)
    vert = mask & (hrun <= max_width) & (vrun >= hrun)

    def collect(m):
        # centre line: for each column, the mean row of each short vertical run
        segs = []
        lab_, k = ndimage.label(m, structure=np.ones((3, 3)))
        for i, sl in enumerate(ndimage.find_objects(lab_), start=1):
            comp = lab_[sl] == i
            rows, cols = np.where(comp)
            if cols.max() - cols.min() < min_len:
                continue
            y = rows.mean() + sl[0].start
            segs.append((y, cols.min() + sl[1].start, cols.max() + sl[1].start))
        return segs

    h = collect(horiz)
    v = [(x, a, b) for (x, a, b) in collect(vert.T)]
    return h, v


def snap_all(values, tol):
    """Map every value to the mean of its cluster (values within tol merge)."""
    order = sorted(set(values))
    groups = [[order[0]]]
    for v in order[1:]:
        if v - groups[-1][-1] <= tol:
            groups[-1].append(v)
        else:
            groups.append([v])
    table = {}
    for g in groups:
        c = float(np.mean(g))
        for v in g:
            table[v] = c
    return table


def to_units(h, v, tol):
    xs_ = [s for _, s, _ in h] + [e for _, _, e in h] + [x for x, _, _ in v]
    ys_ = [y for y, _, _ in h] + [s for _, s, _ in v] + [e for _, _, e in v]
    tx, ty = snap_all(xs_, tol), snap_all(ys_, tol)
    height = crop.shape[0]
    u = lambda px: int(round(px / SCALE))
    hs = sorted({(u(height - ty[y]), u(tx[s]), u(tx[e])) for y, s, e in h if tx[e] > tx[s]})
    vs = sorted({(u(tx[x]), u(height - ty[e]), u(height - ty[s])) for x, s, e in v if ty[e] > ty[s]})
    return hs, vs


wall_h, wall_v = strokes(thin, max_width=12, min_len=20)


def heal(h, v, gap):
    """Pixels where two strokes cross have a long run in both directions and
    were assigned to neither, so every junction leaves a hole of about one
    stroke width. Close those: merge collinear pieces separated by less than
    `gap` px, then extend ends onto perpendicular strokes within `gap` px."""

    def merge(segs):
        # group pieces of the same stroke line (same c within 4 px), then merge along it
        groups = []
        for c, a, b in sorted(segs):
            if groups and abs(groups[-1][-1][0] - c) <= 4:
                groups[-1].append((c, a, b))
            else:
                groups.append([(c, a, b)])
        merged = []
        for grp in groups:
            c = float(np.mean([g[0] for g in grp]))
            for _, a, b in sorted(grp, key=lambda g: g[1]):
                if merged and merged[-1][0] == c and a <= merged[-1][2] + gap:
                    merged[-1] = (c, merged[-1][1], max(merged[-1][2], b))
                else:
                    merged.append((c, a, b))
        return merged

    h, v = merge(h), merge(v)

    def extend(segs, others):
        out_ = []
        for c, a, b in segs:
            for oc, oa, ob in others:
                if oa - gap <= c <= ob + gap:
                    if 0 < a - oc <= gap:
                        a = oc
                    if 0 < oc - b <= gap:
                        b = oc
            out_.append((c, a, b))
        return out_

    return extend(h, v), extend(v, h)


# The plot's outer frame is the outermost hedge (the walls end on it); it was
# drawn with a finer pen than the walls and is added explicitly here.
fh, fw = crop.shape
frame_h = [(pad, pad, fw - 1 - pad), (fh - 1 - pad, pad, fw - 1 - pad)]
frame_v = [(pad, pad, fh - 1 - pad), (fw - 1 - pad, pad, fh - 1 - pad)]
wall_h, wall_v = heal(wall_h + frame_h, wall_v + frame_v, gap=14)
walls_h, walls_v = to_units(wall_h, wall_v, tol=5)
path_h, path_v = strokes(thick, max_width=40, min_len=45)
p_h, p_v = to_units(path_h, path_v, tol=8)
W, H = int(round(crop.shape[1] / SCALE)), int(round(crop.shape[0] / SCALE))
print(f"walls: {len(walls_h)} horizontal, {len(walls_v)} vertical; path: {len(p_h)} + {len(p_v)} pieces; grid {W} x {H}", file=sys.stderr)

out.parent.mkdir(parents=True, exist_ok=True)
with out.open("w") as f:
    f.write("# Hightower's Hampton Court maze, traced from the 1969 paper, page 19 (scripts/trace_maze.py).\n")
    f.write(f"# grid {W} {H}\n")
    f.write("# walls: 'h y x1 x2' and 'v x y1 y2'; Hightower's plotted path: 'ph' / 'pv'\n")
    for y, a, b in walls_h:
        f.write(f"h {y} {a} {b}\n")
    for x, a, b in walls_v:
        f.write(f"v {x} {a} {b}\n")
    for y, a, b in p_h:
        f.write(f"ph {y} {a} {b}\n")
    for x, a, b in p_v:
        f.write(f"pv {x} {a} {b}\n")
print(f"wrote {out}", file=sys.stderr)

# preview
preview = out.with_suffix(".svg")
s = 2.0
with preview.open("w") as f:
    f.write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W*s+20}" height="{H*s+20}"><rect width="100%" height="100%" fill="white"/>\n')
    for y, a, b in p_h:
        f.write(f'<line x1="{a*s+10}" y1="{(H-y)*s+10}" x2="{b*s+10}" y2="{(H-y)*s+10}" stroke="#f4a3a3" stroke-width="6"/>\n')
    for x, a, b in p_v:
        f.write(f'<line x1="{x*s+10}" y1="{(H-a)*s+10}" x2="{x*s+10}" y2="{(H-b)*s+10}" stroke="#f4a3a3" stroke-width="6"/>\n')
    for y, a, b in walls_h:
        f.write(f'<line x1="{a*s+10}" y1="{(H-y)*s+10}" x2="{b*s+10}" y2="{(H-y)*s+10}" stroke="black" stroke-width="2"/>\n')
    for x, a, b in walls_v:
        f.write(f'<line x1="{x*s+10}" y1="{(H-a)*s+10}" x2="{x*s+10}" y2="{(H-b)*s+10}" stroke="black" stroke-width="2"/>\n')
    f.write("</svg>\n")

# --- terminals: the two free ends of the plotted path (A and B)
pieces = [("h", y, a, b) for y, a, b in p_h] + [("v", x, a, b) for x, a, b in p_v]
ends = []
for o, c, a, b in pieces:
    ends += [(a, c), (b, c)] if o == "h" else [(c, a), (c, b)]


def near_other(pt, own):
    for o, c, a, b in pieces:
        if (o, c, a, b) == own:
            continue
        # the thick stroke is ~8 units wide, so pieces stop ~4 units short of a corner
        if o == "h" and abs(pt[1] - c) <= 10 and a - 10 <= pt[0] <= b + 10:
            return True
        if o == "v" and abs(pt[0] - c) <= 10 and a - 10 <= pt[1] <= b + 10:
            return True
    return False


free = []
for o, c, a, b in pieces:
    for pt in ([(a, c), (b, c)] if o == "h" else [(c, a), (c, b)]):
        if not near_other(pt, (o, c, a, b)):
            free.append(pt)
print(f"free path ends (A and B candidates): {free}", file=sys.stderr)
# A is the entrance at the bottom edge, B the end closest to the centre of the maze
a_pt = min(free, key=lambda q: q[1])
centre = (W / 2, H / 2)
b_pt = min((q for q in free if q != a_pt), key=lambda q: (q[0] - centre[0]) ** 2 + (q[1] - centre[1]) ** 2)
if len(free) != 2:
    print("warning: expected exactly two free ends, picked A/B heuristically", file=sys.stderr)
with out.open("a") as f:
    f.write(f"A {a_pt[0]} {a_pt[1]}\nB {b_pt[0]} {b_pt[1]}\n")
