# Implementation Plan: Hightower's Line-Search Routing Algorithm (Rust)

This plan describes how to implement the line-routing algorithm from
D. W. Hightower, *"A Solution to Line-Routing Problems on the Continuous Plane"*,
DAC 1969 (`hightower.pdf` in this directory, DOI: 10.1145/800260.809014),
as a small, dependency-free Rust crate with an SVG visualization module.

The plan is self-contained: everything needed to implement the algorithm is
specified below; the paper is only needed for cross-checking.

**Language choice: Rust.** The algorithm's selling point is speed and low memory
(it stores only line segments, no grid), which Rust preserves. A trace/SVG module
will generate figures for an accompanying blog post, and the crate can later be
compiled to WASM for an interactive demo. Integer coordinates (`i64`) are used
throughout — no floating point — so all intersection tests are exact.

**Intended application** (from `hightower_slides.pdf`): routing orthogonal edges
around boxes in diagrams / graph layouts. Fast, mostly-straight paths matter more
than shortest paths; a fallback (e.g. A*) handles the rare failures. The crate is
a standalone library, but this context informs API decisions (e.g. multi-net mode,
trace events).

---

## 1. Problem statement

Given:

- a rectangular bounding box,
- a set `C_h` of non-traversable **horizontal** segments and a set `C_v` of
  non-traversable **vertical** segments (obstacles; rectangles are entered as
  their 4 edges),
- two points `A` and `B` (not on any obstacle, inside the bounds),
- a **unit** = 1: the minimum clearance between the path and any obstacle,

find a **rectilinear path** (only axis-parallel segments, right-angle bends) from
`A` to `B` that does not intersect any obstacle segment, or report failure.

Properties of the algorithm (document these in the crate docs):

- Very fast and memory-light: it only ever constructs line segments, no grid.
- Paths tend to have few bends, but are **not guaranteed shortest**.
- It is **incomplete**: it can fail to find a path that exists (it never reuses an
  escape line, so e.g. it may not get back out of a box entered through a narrow
  mouth). Callers must handle `None` even for connected instances.

---

## 2. Core concepts (precise definitions)

All coordinates are `i64`. The unit is `1`.

- **Segment**: axis-parallel; represented as orientation + fixed coordinate +
  span `[from, to]` with `from <= to`. A point is a zero-length segment
  (`from == to`) — this must be supported everywhere.

- **Cover** (verb): a segment `s` *covers* point `p` if the perpendicular from `p`
  onto the line containing `s` hits `s`. Concretely: a horizontal segment
  `{y = c, x in [x1, x2]}` covers `p` iff `x1 <= p.x <= x2`. A vertical segment
  `{x = c, y in [y1, y2]}` covers `p` iff `y1 <= p.y <= y2`.

- **Horizontal covers of p** (noun): among all horizontal obstacle segments that
  cover `p`, the nearest one strictly above `p` and the nearest one strictly
  below `p` (either may be absent). **Vertical covers of p**: nearest covering
  vertical segments strictly left / strictly right of `p`.

- **Escape lines of p**:
  - the **vertical escape line** through `p` is the segment
    `{x = p.x, y in [lo, hi]}` where `hi = (y of horizontal cover above) - 1`
    (or the top bound of the box if no cover above) and
    `lo = (y of horizontal cover below) + 1` (or the bottom bound).
  - the **horizontal escape line** through `p` is defined symmetrically, bounded
    by the vertical covers of `p` (∓ 1 unit) or the box.

  Note: the paper bounds escape lines *on* the covers and later steps one unit
  inward; bounding them one unit *short of* the covers up front is equivalent and
  simpler, and automatically gives the path a clearance of 1 unit. The box bounds
  are inclusive (a line may end exactly on the boundary).

  An escape line can have zero length (covers at distance 1 on both sides); this
  is legal and must not panic.

- **Escape point**: a point on an escape line of the current *object point* `Z`
  from which a *new, useful, unused* perpendicular escape line can be drawn.
  Made precise in the escape processes below.

- **Used line**: each network (see below) records every escape line it has
  constructed. A candidate line (orientation, fixed coordinate, span) counts as
  *used* if the same network already contains a line with the same orientation,
  the same fixed coordinate, and an **overlapping span**. The algorithm never
  constructs a used line — this is what guarantees progress and termination, and
  is also the root cause of the incompleteness (documented above).

---

## 3. Algorithm specification

Two **networks** grow simultaneously, one rooted at `A`, one at `B`. Each network
stores:

```text
Network {
    root: Point,
    escape_points: Vec<PointId>,      // stack; last = current object point Z
    lines: Vec<EscapeLine>,           // all constructed lines, both orientations
    orientation_flag: Both | Horizontal | Vertical,   // orientation of the NEXT line
    no_escape: bool,
}

EscapeLine {
    orientation: Horizontal | Vertical,
    fixed: i64,                       // y for horizontal, x for vertical
    span: (i64, i64),                 // from <= to
    through: Point,                   // the escape point it was drawn through
    parent: Option<LineId>,           // line that `through` was found on (None for seed lines)
}
```

### 3.1 Main procedure

```text
route(A, B) -> Option<Vec<Point>>:
    validate: A != B not required (A == B -> Some([A])); A, B inside bounds and
              not covered-at-distance-0 (i.e. not lying ON any obstacle segment),
              otherwise return None.
    netA = Network(A); netB = Network(B); both orientation_flag = Both.
    current = netA, other = netB
    loop (with a global iteration cap, default 10_000 escape steps; cap exceeded -> None):
        if current.no_escape and other.no_escape: return None
        if current.no_escape: swap(current, other); continue
        result = escape_step(current, other)
        if result == Intersection(x_point, line_current, line_other):
            return Some(reconstruct_and_refine(x_point, line_current, line_other))
        swap(current, other)
```

### 3.2 One escape step

```text
escape_step(net, other) -> Intersection | Continue | NoEscape:
    Z = net.escape_points.last()
    // 1. construct escape line(s) through Z per orientation flag
    for each orientation in (net.orientation_flag == Both ? [H, V] : [flag]):
        if the escape line through Z with this orientation is not used in `net`:
            add it (through = Z, parent = line Z was found on)
            // 2. intersection test against the other network
            for each line L of `other` with perpendicular orientation:
                if lines cross (fixed coords inside each other's spans):
                    return Intersection(cross point, new line, L)
    // 3. find the next escape point
    if let Some(e, next_orientation) = escape_process_I(net, Z):
        push e; net.orientation_flag = next_orientation; return Continue
    if let Some(e, next_orientation) = escape_process_II(net, other, Z):
        // process II may itself detect an intersection; see 3.4
        push e; net.orientation_flag = next_orientation; return Continue / Intersection
    net.no_escape = true; return NoEscape
```

Note the seed behavior: the first call for each network has `orientation_flag =
Both` and constructs *both* lines through the root. This makes the trivial cases
work: if `A` and `B` see each other along a clear row/column, the very first
intersection tests already succeed (`B`'s vertical line crosses `A`'s horizontal
line at `(B.x, A.y)` etc.).

### 3.3 Escape Process I — "slip around the end of a cover"

Intuition: the horizontal cover above/below `Z` blocks vertical movement. If `Z`'s
*horizontal* escape line reaches past an end of that cover, we can move sideways
to one unit beyond the cover's end and then travel vertically past it.

```text
escape_process_I(net, Z) -> Option<(Point, Orientation)>:
    // Phase 1: escape vertically around the ends of the HORIZONTAL covers of Z.
    f[1..=4] = the endpoints of the horizontal cover above Z and the horizontal
               cover below Z (0, 2 or 4 points, missing covers contribute none),
               sorted ascending by Euclidean distance to Z.
    for f in f:
        e = (f.x + 1 in the direction AWAY from Z, Z.y)
            // i.e. e.x = f.x + 1 if f.x >= Z.x else f.x - 1
        if e lies on Z's horizontal escape line (within its span)
           and the vertical escape line through e extends strictly past the
               cover that f belongs to (i.e. past that cover's y, in the
               direction from Z toward that cover)
           and that vertical line is not used in `net`:
            return Some(e, Vertical)
    // Phase 2: symmetric — escape horizontally around the ends of the VERTICAL
    // covers of Z; candidates e = (Z.x, f.y ± 1 away from Z); require e on Z's
    // vertical escape line, horizontal line through e extends past f's cover,
    // line unused.
    ...
    return None
```

The "extends strictly past the cover" condition is the practical version of the
paper's escape-point definition D6: the new perpendicular line must reach
territory that the blocking cover was fencing off. (Checking span-overlap
against the used-lines set already prevents useless duplicates; the
"extends past" check prevents accepting a point whose new line is immediately
blocked by another segment between `Z` and the cover.)

### 3.4 Escape Process II — "retreat along the escape line"

Used when Process I fails (typical inside a box, or when covers are longer than
the escape lines). Take the four endpoints of `Z`'s two escape lines and walk
each of them back toward `Z` one unit at a time, round-robin; at every visited
position, try to branch off perpendicularly.

```text
escape_process_II(net, other, Z) -> Option<(Point, Orientation)> | Intersection:
    r[1] = top    end of Z's vertical escape line
    r[2] = right  end of Z's horizontal escape line
    r[3] = bottom end of Z's vertical escape line
    r[4] = left   end of Z's horizontal escape line
    loop over i = 1,2,3,4,1,2,... :
        if all four r[i] coincide with Z: return None
        if r[i] == Z: continue
        L = perpendicular escape line through r[i]
            (horizontal line for i in {1,3}, vertical line for i in {2,4})
        if L not used in `net`:
            construct L (through = r[i], parent = Z's line of matching axis)
            check L against `other`'s perpendicular lines:
                on hit -> push r[i], return Intersection(...)
            if let Some(e, o) = escape_process_I applied at r[i]
                                (using r[i] as object point, its own covers,
                                 its own escape lines):
                push r[i] first, then return Some(e, o)
                // both r[i] and e become escape points, in that order
        move r[i] one unit toward Z (decrement the coordinate that differs)
```

Bound the loop: each `r[i]` strictly approaches `Z`, so it terminates after at
most `sum of the two escape-line lengths` iterations. Also respect the global cap.

### 3.5 Path reconstruction

When lines `La` (network A) and `Lb` (network B) cross at point `X`:

```text
chain(line, X):  // corner candidates from X back to the network root
    pts = [X]
    l = line
    while l is not None:
        pts.push(l.through)
        l = l.parent
    return pts        // ends at the root (seed lines have through == root)

raw_path = reverse(chain(La, X)) ++ chain(Lb, X)[1..]   // A ... X ... B
```

Every consecutive pair in `raw_path` shares an x or y coordinate by
construction (each point lies on its predecessor's line), so the raw path is
rectilinear. It may contain collinear runs and overshoots (this replaces the
paper's "First Refinement", which is only needed because the paper stores flat
point stacks instead of parent pointers).

**Cleanup pass (required):** repeatedly drop any point `p_i` (0 < i < n) where
`p_{i-1}`, `p_i`, `p_{i+1}` are collinear (all same x or all same y), and drop
consecutive duplicates, until a fixed point is reached. Result: a minimal corner
list where consecutive segments alternate orientation.

### 3.6 Second Improvement — path tightening (paper Fig. 8–12)

The raw path often takes detours (paper Fig. 10). Implement **Improvement A**
(required); **Improvement B** is a stretch goal.

**Improvement A — segment extension shortcut** (Fig. 10 → 11): for each segment
`(p_i, p_i+1)` of the path, compute the *maximal* obstacle-free extension of that
segment: the escape line through `p_i` with the segment's orientation (already
implemented machinery). If that line intersects a *later, perpendicular* path
segment `(p_j, p_j+1)` with `j >= i+3` (check `j = i+3, i+5, ...`), let `X` be
the intersection point, delete `p_{i+2} .. p_j`, and set `p_{i+1} = X`. Restart
the scan (or continue with adjusted indices, as in the paper's flowchart). Run
the whole pass twice: once on `A..B`, once on the reversed path.

**Improvement B — perpendicular probing** (Fig. 8 → 9, stretch goal): for each
segment, slide a probe point `q` along it at unit steps starting from the far
end; construct the escape line through `q` perpendicular to the segment; if it
intersects a later perpendicular path segment at `X`, splice `q` and `X` in and
delete the points between. This removes staircases. Only implement after
everything else works; keep it behind a `RouterConfig` option.

After any improvement pass, run the collinear cleanup again, then `debug_assert!`
path validity (see §6).

---

## 4. Crate design

```
hightower/
├── Cargo.toml            # edition 2024, no runtime dependencies
├── plan.md, blog.md, hightower.pdf, hightower_slides.pdf   (already present)
├── src/
│   ├── lib.rs            # re-exports, crate docs (incl. properties/caveats)
│   ├── geometry.rs       # Point, Orientation, Segment, intersection helpers
│   ├── obstacles.rs      # ObstacleSet: storage + cover queries
│   ├── router.rs         # Network, main loop, escape step (§3.1, 3.2)
│   ├── escape.rs         # Process I and Process II (§3.3, 3.4)
│   ├── refine.rs         # reconstruction, cleanup, improvements (§3.5, 3.6)
│   ├── trace.rs          # TraceEvent recording
│   └── svg.rs            # SVG rendering of scenes and traces
├── examples/
│   ├── demo.rs           # routes a handful of scenes, writes out/*.svg
│   ├── animate.rs        # writes one SVG per trace event (blog animation frames)
│   └── maze.rs           # Hampton-Court-style maze (paper p.19 homage)
└── tests/
    ├── routing.rs        # scenario tests (§6)
    └── properties.rs     # randomized property tests (§6)
```

### 4.1 Public API

```rust
pub type Coord = i64;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Point { pub x: Coord, pub y: Coord }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation { Horizontal, Vertical }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Segment { pub orientation: Orientation, pub fixed: Coord, pub from: Coord, pub to: Coord }

pub struct Bounds { pub min: Point, pub max: Point }

pub struct ObstacleSet { /* see 4.2 */ }
impl ObstacleSet {
    pub fn new(bounds: Bounds) -> Self;
    pub fn add_segment(&mut self, s: Segment);
    pub fn add_rect(&mut self, min: Point, max: Point);   // 4 segments
    pub fn add_path(&mut self, corners: &[Point]);        // for multi-net mode
}

#[derive(Clone, Debug)]
pub struct RouterConfig {
    pub max_steps: usize,          // default 10_000
    pub improve: Improvement,      // None | ExtensionOnly (default) | Full
}

pub fn route(obstacles: &ObstacleSet, a: Point, b: Point) -> Option<Vec<Point>>;
pub fn route_with(obstacles: &ObstacleSet, a: Point, b: Point,
                  config: &RouterConfig) -> RouteResult;

pub struct RouteResult {
    pub path: Option<Vec<Point>>,  // corner list, A first, B last
    pub trace: Trace,              // every event, for visualization/debugging
}
```

`Vec<Point>` is the corner list including endpoints; consecutive corners differ
in exactly one coordinate.

### 4.2 ObstacleSet internals & cover queries

Per the notes in `hightower_slides.pdf`: keep horizontal segments in a
`BTreeMap<Coord /* y */, Vec<(Coord, Coord) /* x-span */>>` and vertical segments
in the symmetric map (spans kept sorted, merged if overlapping is *not* required —
duplicates are fine).

- `cover_above(p) -> Option<Segment>`: iterate `map.range(p.y + 1 ..)` in
  ascending order; at each y, binary-search/scan the spans for one containing
  `p.x`; first hit wins. `cover_below`, `cover_left`, `cover_right` symmetric.
  Worst case O(rows), fine for the target scale (thousands of segments); an
  interval tree is an explicitly out-of-scope optimization.
- `escape_line(p, orientation) -> Segment` built from the covers ∓ 1 and the
  bounds (§2).
- `is_on_obstacle(p) -> bool` for input validation (check the maps at exactly
  `p.y` / `p.x`).

The four bounding-box edges are **not** stored as obstacles; bounds clip escape
lines directly (equivalent, avoids special cases at corners).

### 4.3 Trace and SVG

`TraceEvent` (one enum, chronological `Vec<TraceEvent>` in `Trace`):

```rust
enum TraceEvent {
    LineAdded    { net: NetId /* A | B */, line: EscapeLineInfo },
    EscapePoint  { net: NetId, point: Point, process: I | II },
    Intersection { point: Point },
    RawPath      { corners: Vec<Point> },
    Improved     { corners: Vec<Point> },   // after each improvement pass
}
```

`svg.rs` renders a scene + trace prefix to an SVG string:

- obstacles: black, thick; bounds: thin gray frame
- network A lines: blue; network B lines: green (matches the hand-drawn slides)
- escape points: filled dots in the network color
- intersection: orange circle
- final path: red, thick
- `render(scene, trace, upto: usize) -> String` so `examples/animate.rs` can dump
  frame-by-frame SVGs (`out/frame_000.svg`, …) for the blog post; also a
  convenience `render_final`.

No external crates — SVG is written with `format!`/`write!`. Flip the y-axis at
render time (SVG y grows downward).

---

## 5. Milestones (implement in this order)

Each milestone compiles, passes `cargo test` and `cargo clippy` before moving on.

1. **M1 — Geometry & obstacles**: `geometry.rs`, `obstacles.rs`; cover queries,
   escape-line construction, segment-crossing predicate. Unit tests for covers
   (incl. point exactly beside a segment end, no-cover cases, zero-length lines).
2. **M2 — Networks & main loop, Process I only**: seed lines, intersection
   detection, escape step, used-line bookkeeping. Tests: direct line-of-sight
   cases and a single wall between A and B (path with 2 bends around either end).
3. **M3 — Path reconstruction + cleanup** (§3.5): valid corner lists for all M2
   scenarios; the path-validity checker (§6) lands here and is used everywhere.
4. **M4 — Process II**: test scene "B inside a 3-sided box, opening away from A"
   (topology of paper Fig. 14) → path threads the opening. Test "B inside a
   closed box" → `None` via both no-escape flags, not via the step cap.
5. **M5 — Improvement A** (§3.6): before/after bend-count assertions on a scene
   with a known detour; validity preserved.
6. **M6 — Trace + SVG + examples**: `demo.rs`, `animate.rs`, `maze.rs` produce
   SVGs into `out/` (gitignored). Eyeball them.
7. **M7 — Property tests, docs, polish**: §6 property tests, rustdoc for the
   public API including the incompleteness caveat, README with one embedded
   example. Optional: `Improvement B`, criterion benchmark vs. a naive grid-BFS
   (Lee-style) router in `benches/` — nice blog material, not required.

Bootstrap: `cargo init --lib` in this directory (`git init` first — the directory
is not yet a repository; add `out/` and `target/` to `.gitignore`; keep the two
PDFs and the two `.md` files).

## 6. Testing strategy

**Path validity checker** (test util, also used in `debug_assert!`s):
`assert_valid(obstacles, a, b, path)` checks

- `path.first() == a`, `path.last() == b`,
- consecutive corners differ in exactly one coordinate (no zero-length segments
  after cleanup, no diagonal moves),
- no path segment intersects or touches any obstacle segment (exact integer
  overlap tests: crossing, T-touch, collinear overlap — all forbidden),
- all corners within bounds.

**Scenario tests** (`tests/routing.rs`), bounds `(0,0)–(100,100)` unless noted:

| # | Scene | Expectation |
|---|-------|-------------|
| 1 | no obstacles, `A=(10,10)`, `B=(80,60)` | path found, ≤ 2 bends |
| 2 | `A`, `B` on the same row, clear | path found, 0–2 bends after cleanup |
| 3 | single vertical wall `x=50, y∈[0,80]` between them | path with ≥ 2 bends over the top |
| 4 | wall spanning the full height | routes around? No: full-height wall touching both bounds ⇒ `None` |
| 5 | Fig.-14 topology: `A=(10,10)`; wall `x=15, y∈[9,21]` next to A; box around `B=(38,25)`: top `y=30,x∈[31,46]`, left `x=31,y∈[20,30]`, right `x=46,y∈[20,30]`, bottom `y=20,x∈[36,46]` (opening at bottom-left `x∈[31,36]`) | path found through the opening, valid |
| 6 | `B` in a fully closed box | `None`, and both networks ended with `no_escape` (expose this in `RouteResult` for the test) |
| 7 | `A == B` | `Some([A])` |
| 8 | `A` on an obstacle segment | `None` |
| 9 | box with a 3-unit mouth and a baffle behind it (known-incompleteness probe) | document actual behavior; if it fails to find the existing path, keep as `#[test]` asserting `None` with a comment — this is blog material, not a bug |

**Property tests** (`tests/properties.rs`, seeded `SmallRng`-style PRNG written
by hand or `proptest` as a dev-dependency — either is fine):

- generate 0–40 random axis-parallel segments (length 0–30) and random `A`, `B`
  not on obstacles, in a 128×128 box; for each seed:
  - `route` terminates within the cap (no panic, no overflow),
  - if `Some(path)`: `assert_valid`,
  - soundness cross-check: rasterize obstacles onto a unit grid (cells blocked if
    an obstacle passes through/borders them) and run BFS; whenever Hightower
    returns `Some`, BFS must also reach `B`. (No assertion in the other
    direction — the algorithm is legitimately incomplete.)
- run a few hundred seeds in CI-time budget.

## 7. Pitfalls & edge cases (read before coding)

- **Off-by-one discipline**: escape lines end at `cover ∓ 1`; Process I
  candidates sit at `endpoint ± 1` *away* from `Z`; Process II steps *toward*
  `Z`. Write tiny helper functions (`toward(a, b)`, `away(a, b)`) instead of
  inlining sign fiddling.
- **Zero-length escape lines** (span `(c, c)`) are valid lines through `Z`; the
  crossing predicate must treat them as points.
- **Used-line rule** must compare *span overlap*, not just the fixed coordinate:
  two vertical lines at the same `x` separated by an obstacle are different
  lines and both allowed.
- **Intersection at line endpoints** is a valid intersection (spans are
  inclusive).
- **Process II bookkeeping**: when Process I succeeds *at* `r_i`, push `r_i`
  first and then `e` — two escape points from one step; the parent-line pointers
  must reflect this (e's line's parent is the line through `r_i`).
- **Orientation flag** is per-network state and persists across turns; only
  Process I/II set it (to the orientation of the *next* line to construct).
- **Alternation**: strictly alternate networks each step (paper's main
  procedure); when one network is `no_escape`, keep stepping the other alone.
- **`i64` everywhere**; no floats. Euclidean distance for sorting endpoint
  candidates may use `i128` squared distances to avoid overflow.
- **Cap exceeded** returns `None` but should be distinguishable in `RouteResult`
  (e.g. `enum Outcome { Found, NoEscape, StepLimit }`) — the blog will want that.

## 8. Definition of done

- [ ] `cargo test` green, incl. property tests; `cargo clippy -- -D warnings` clean; `cargo fmt` applied.
- [ ] `route` handles all scenario tests as specified.
- [ ] `cargo run --example demo` writes valid SVGs showing obstacles, both
      networks, and the final path; `animate` writes per-step frames.
- [ ] Public API documented (`cargo doc` without warnings), incl. explicit
      "not shortest, not complete" caveats and a doc example.
- [ ] No runtime dependencies; dev-dependencies at most `proptest`/`criterion`.
