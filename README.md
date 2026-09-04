# hightower

Hightower's line-search routing algorithm (1969) in Rust: fast rectilinear
paths around axis-parallel obstacles, without a grid.

D. W. Hightower, *A Solution to Line-Routing Problems on the Continuous Plane*,
DAC 1969, [doi:10.1145/800260.809014](https://doi.org/10.1145/800260.809014).

Two networks of *escape lines* grow alternately from the start and the target
point. An escape line is the longest obstacle-free horizontal or vertical
segment through a point; new lines branch off around the ends of the obstacles
that stopped the old ones. As soon as a line of one network crosses a line of
the other, a path exists and is read off the two trees.

![two networks meeting](out/blog/08_full_run.svg)

## Properties

* **Fast and memory-light.** Only line segments are ever constructed; there is
  no grid, so the cost depends on the clutter, not on the area. Routing a
  20-box scene takes about 1–2 µs regardless of whether the board is 64 or
  2048 units wide (see `examples/bench.rs`).
* **Few bends, not shortest.** Paths tend to be straight and calm, but they
  are not guaranteed to be shortest.
* **Not complete.** Escape lines are never reused. That guarantees termination
  but means a path can be missed even though it exists. Callers must handle
  `None` for connected instances, e.g. by falling back to the visibility-graph
  router below.
* **Exact.** All coordinates are `i64`; one unit is the minimum clearance
  between the path and any obstacle. No floating point, no dependencies.

## Usage

```rust
use hightower::{Bounds, ObstacleSet, Point, route};

let mut obstacles = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(100, 100)));
obstacles.add_rect(Point::new(40, 20), Point::new(60, 80));

let path = route(&obstacles, Point::new(10, 50), Point::new(90, 50));
// Some([(10,50), (10,81), (90,81), (90,50)]) — a corner list, A first, B last
```

`route_with` returns a `RouteResult` with the outcome (`Found`, `NoEscape`,
`StepLimit`, `InvalidInput`), the number of steps and a `Trace` of every escape
line, escape point and intersection. The `svg` module renders scenes and traces
(or any prefix of a trace, for animations).

```rust
use hightower::{RouterConfig, Improvement, route_with};
use hightower::svg::{Scene, Style, render_final};

let config = RouterConfig { improve: Improvement::Full, ..Default::default() };
let result = route_with(&obstacles, a, b, &config);
let svg = render_final(&Scene { obstacles: &obstacles, a, b }, &result.trace, &Style::default());
```

Already routed paths can be added as obstacles (`add_path`) or only their
corners (`add_path_corners`, the paper's PERT-diagram mode) before routing the
next connection.

## Reference routers

Two more routers solve the same problem and serve as oracle, fallback and
benchmark baseline:

* `hightower::route_visibility` – A\* over the **orthogonal visibility graph**
  (Wybrow, Marriott, Stuckey, *Orthogonal Connector Routing*, GD 2009; the
  approach behind libavoid). The lines through every obstacle side (pushed one
  unit outward) and through both endpoints form a sparse grid; A\* searches it
  without materialising it. Complete, grid-free, and optimal for
  `length + bend_penalty * bends` (`VisibilityConfig`). With `bend_penalty: 0`
  it returns a shortest path. Roughly 5–10× slower than Hightower on typical
  diagram scenes, but it never misses a path.
* `hightower::grid::route_grid` – a naive Lee-style BFS on the unit lattice.
  Complete and shortest, but cost grows with the area. Used as the oracle in
  the property tests.

```rust
use hightower::{VisibilityConfig, route_visibility_with};

let calm = route_visibility_with(&obstacles, a, b, &VisibilityConfig { bend_penalty: 20 });
// calm.path: fewest-bend route for that trade-off; calm.cost, calm.expanded for diagnostics
```

## Examples

```sh
cargo run --release --example demo            # a few scenes -> out/demo_*.svg
cargo run --release --example animate         # one SVG per trace event -> out/frames/
cargo run --release --example maze            # Hightower's Hampton Court maze -> out/maze.svg
cargo run --release --example counterexample  # searches scenes the router cannot solve
cargo run --release --example blog_figures    # all figures of the blog post -> out/blog/
cargo run --release --example bench           # Hightower vs. grid BFS vs. visibility graph -> out/bench.csv
python3 scripts/plot_bench.py                 # -> out/blog/12_benchmark.svg
```

## Implementation notes

* `geometry.rs` – points, axis-parallel segments, exact crossing tests.
* `obstacles.rs` – `ObstacleSet`: horizontal and vertical segments in
  `BTreeMap`s keyed by their fixed coordinate; cover queries walk the map
  outward from the query point; `escape_line` stops one unit short of the
  covers (that is where the clearance comes from).
* `router.rs` – the two networks, the main loop and one escape step.
* `escape.rs` – Escape Process I (slip around the end of a cover) and
  Process II (retreat along the escape line and branch off, recursively).
* `refine.rs` – path reconstruction from the escape-point trees, collinear
  cleanup, the paper's second improvement (segment extension, optional
  perpendicular probing) and a validity checker.
* `visibility.rs` – orthogonal visibility graph plus A\* (complete, shortest
  or bend-optimised); the modern reference and the recommended fallback.
* `grid.rs` – a naive Lee-style BFS used as oracle in tests and as the
  baseline in the benchmark.

Three deliberate deviations from the plan in `references/plan.md`: escape
points carry parent pointers (a tree) instead of lines, which makes the
paper's "first refinement" unnecessary; Process II only retreats from
escape-line ends that were stopped by a cover, not from ends on the bounding
box, as in the paper (the more thorough variant is
`RouterConfig::boundary_retreat`); and Process II retreats recursively along
the probe lines it constructs (`RouterConfig::recursive_retreat`, default
on). The last one follows from the paper's wording and is what makes the
router solve Hightower's own Hampton Court maze, which the flat reading
cannot. On 16782 random room-and-door scenes the default misses about 2 %
of the existing paths, the flat reading 2.3 %, and `boundary_retreat` 0.1 %.

## The Hampton Court maze

`examples/data/hampton_court.txt` holds the maze from page 19 of the paper,
traced from the scan with `scripts/trace_maze.py` (walls, Hightower's
plotted path, A and B). `cargo run --release --example maze` routes it and
reports the time the way the plot did, in hours:

```
SOLUTION TO HAMPTON COURT MAZE
FOUND PATH FROM A TO B
TOTAL TIME .0000005
```

The raw path of the recursive retreat zigzags from wall to wall in the
corridors (5095 units here); the default `Improvement::Full` removes that
and ends up at the shortest length (1357 units).

![the traced maze with both paths](blog/images/03_maze.svg)

## Tests

```sh
cargo test
```

Scenario tests in `tests/routing.rs` follow the plan; `tests/properties.rs`
routes hundreds of random scenes and checks every result against the grid
BFS: whenever Hightower returns a path, it is valid and BFS agrees that one
exists, and the visibility-graph router finds a path exactly when BFS does,
with the same length.

## License

MIT or Apache-2.0, at your option.
