#!/usr/bin/env python3
"""Plots out/bench.csv (written by `cargo run --release --example bench`)
as two small multiples: run time over board side and over obstacle count."""

import csv
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

BLUE = "#1f6fd6"   # Hightower (same blue as network A in the SVG figures)
RED = "#d62828"    # grid BFS (same red as the final path)
INK = "#0b0b0b"
INK2 = "#52514e"
GRID = "#e6e6e3"
SURFACE = "#ffffff"

src = Path(sys.argv[1] if len(sys.argv) > 1 else "out/bench.csv")
dst = Path(sys.argv[2] if len(sys.argv) > 2 else "out/blog/12_benchmark.svg")
rows = list(csv.DictReader(src.open()))
area = [r for r in rows if r["series"] == "area"]
clutter = [r for r in rows if r["series"] == "clutter"]

plt.rcParams.update({
    "font.family": "sans-serif",
    "font.size": 11,
    "axes.edgecolor": GRID,
    "axes.labelcolor": INK2,
    "xtick.color": INK2,
    "ytick.color": INK2,
    "text.color": INK,
})

fig, axes = plt.subplots(1, 2, figsize=(10, 4.2), facecolor=SURFACE)


def style(ax, title, xlabel):
    ax.set_facecolor(SURFACE)
    ax.set_yscale("log")
    ax.grid(True, which="major", axis="y", color=GRID, linewidth=1)
    ax.grid(False, axis="x")
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    ax.tick_params(length=0)
    ax.set_title(title, loc="left", fontsize=12, color=INK, pad=10)
    ax.set_xlabel(xlabel)
    ax.set_ylabel("Laufzeit pro Route")
    ax.set_yticks([1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8])
    ax.set_yticklabels(["100 ns", "1 µs", "10 µs", "100 µs", "1 ms", "10 ms", "100 ms"])
    ax.set_ylim(1e2, 3e8)


def series(ax, data, xkey):
    xs = [int(r[xkey]) for r in data]
    h = [int(r["hightower_ns"]) for r in data]
    g = [int(r["grid_ns"]) for r in data]
    ax.plot(xs, g, color=RED, linewidth=2, marker="o", markersize=6, markeredgecolor=SURFACE, markeredgewidth=1.5)
    ax.plot(xs, h, color=BLUE, linewidth=2, marker="o", markersize=6, markeredgecolor=SURFACE, markeredgewidth=1.5)
    ax.annotate("Gitter-BFS (Lee)", (xs[0], g[0]), xytext=(0, 12), textcoords="offset points",
                ha="left", color=INK, fontsize=11)
    ax.annotate("Hightower", (xs[0], h[0]), xytext=(0, -20), textcoords="offset points",
                ha="left", color=INK, fontsize=11)


style(axes[0], "20 Hindernisse, wachsende Fläche", "Kantenlänge des Spielfelds")
axes[0].set_xscale("log", base=2)
axes[0].set_xticks([int(r["side"]) for r in area])
axes[0].set_xticklabels([r["side"] for r in area])
series(axes[0], area, "side")

style(axes[1], "Fläche 256 × 256, wachsende Anzahl Hindernisse", "Anzahl Rechtecke")
axes[1].set_xscale("symlog", base=2, linthresh=5)
axes[1].set_xticks([int(r["obstacles"]) for r in clutter])
axes[1].set_xticklabels([r["obstacles"] for r in clutter])
axes[1].set_ylabel("")
series(axes[1], clutter, "obstacles")

fig.tight_layout(w_pad=3)
dst.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(dst, facecolor=SURFACE)
fig.savefig(dst.with_suffix(".png"), dpi=150, facecolor=SURFACE)
print(dst)
