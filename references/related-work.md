# Related Work: What Replaced Line Search?

Research notes (September 2026) on the question: are there more modern, better,
or more widely used algorithms for the problem Hightower's 1969 line-search
router solves — rectilinear paths around axis-parallel obstacles, without a grid?

Short answer: yes, and the successor is not a better line-search variant. It is
**sparse graph construction plus A\***. Hightower's core insight (only obstacle
corners and their extensions matter; the grid in between is waste) survived; his
greedy two-net search did not.

## 1. The immediate family (1968–1978, historical only)

| Algorithm | Year | Idea | Complete? | Shortest? |
|---|---|---|---|---|
| Lee | 1961 | BFS wavefront on a grid | yes | yes |
| Mikami–Tabuchi | 1968 | trial lines from *every* grid point on a line | yes | no |
| **Hightower** | 1969 | trial lines only from escape points past blocking covers | **no** | no |
| Hadlock | 1977 | maze search with detour numbering — effectively A\* before A\* | yes | yes |
| Soukup | 1978 | hybrid: line search toward the target, BFS when stuck | yes | no |

The trade-off between Mikami–Tabuchi (complete, many lines) and Hightower
(cheap, incomplete) is exactly why neither ships in pure form today. Standard
textbook treatments (Chang, NTU; Chen/Chang chapter on global and detailed
routing) present the family as history.

Measured here: on 16782 solvable random room-and-door scenes, this crate's
Hightower implementation missed 428 paths (~2.5%); the border-retreat variant
drops that to ~0.5% at the cost of many more lines.

## 2. The actual successor: orthogonal visibility graph + A\*

The most relevant line of work for this crate's target use case (interactive
diagram editor).

**Wybrow, Marriott, Stuckey, *Orthogonal Connector Routing*, Graph Drawing 2009**
— implemented in **libavoid** (part of Adaptagrams).

Construction: take the horizontal and vertical lines of visibility from every
obstacle corner and connector port. Their intersections form a graph (in essence
a Hanan grid over the obstacle coordinates). Search it with A\*.

Properties, contrasted with Hightower:

* **Complete** — finds a route whenever one exists. The 2.5% blind spot is gone.
* **Optimal** w.r.t. a monotonic cost function over *both* length and bend count.
  This is the same objective the blog article argues for ("shortest is the wrong
  goal"), but obtained by construction rather than by post-hoc improvement passes.
* **Grid-free** — cost scales with obstacle count, not with board area. Same
  asymptotic advantage over Lee/BFS that Hightower has.
* **Incremental** — a companion paper covers rerouting during interaction.

Price: the graph has O(n²) nodes for n obstacles in the worst case. Irrelevant at
20 rectangles, noticeable at 320.

**Marriott, Stuckey, Wybrow, *Seeing Around Corners: Fast Orthogonal Connector
Routing*, Diagrams 2014** — speeds this up. Observation: many routes are
topologically equivalent, so the search wastes work on them. The paper introduces
*obstacle-hugging routes* as a conjectured canonical representative per
equivalence class, and a **1-bend visibility graph** that supports computing them.

Deployment: libavoid is used by Inkscape, Gaphas, BRL-CAD, Dunnart, Arcadia and
commercial circuit-diagram editors. yFiles and Eclipse ELK use comparable
approaches for orthogonal edge routing. This is the de-facto standard for
diagram editors.

Recent graph-drawing work (Hegemann & Wolff, *A Simple Pipeline for Orthogonal
Graph Drawing*, GD 2023) targets full orthogonal layout rather than
fixed-node edge routing, but contains an LP-based edge-nudging step that is
relevant for bundling parallel routes.

## 3. Theory

The exact problem is solved:

* Rectilinear shortest path among *rectangular* obstacles: O(n log n)
  preprocessing, O(n) space, O(log n + k) query for a path with k turns.
* Clarkson, Kapoor, Vaidya (SoCG 1987): rectilinear shortest paths through
  *polygonal* obstacles in O(n log² n).
* Continuous-Dijkstra formulations exist, including Θ(n log n) for transient
  obstacles; minimum-link path variants are studied separately.

Practical implementations still prefer the visibility-graph route because it is
simpler and handles the bend-vs-length objective naturally.

## 4. EDA today: the problem moved

In chip and board design, "how do I route one net" stopped being the bottleneck
decades ago. The modern structure is two-stage — **global routing** on a coarse
tile graph, then **detailed routing** — and the actual algorithm is **rip-up &
reroute with negotiated congestion**: nets are allowed to overlap, congested
resources get progressively more expensive, everything is re-routed iteratively.

Open-source reference points:

* **TritonRoute / TritonRoute-WXL** (UCSD) — detailed router in OpenROAD.
  Pin access analysis, track assignment, initial detailed routing, search &
  repair, DRC engine; iterative A\* over partitions with dynamic boundary
  adjustment. WXL reports ~99.99% fewer DRCs than the CUGR + Dr. CU 2.0 flow at
  comparable wirelength, via count and runtime.
* **Dr. CU** — Dijkstra on a sparse grid graph with partitioning.
* **CUGR / CUGR2** — global routing.

2024/25 research directions: machine learning and reinforcement learning to
accelerate routing convergence (XRoute Environment; offline-RL for detailed
routing), plus GPU parallelisation and hybrid metaheuristics.

PCB specifically: **Freerouting** is the widely used open-source autorouter
(Specctra DSN interface, drives KiCad via export/import); KiCad's own interactive
router is **push-and-shove** rather than pure search. Line search survives only as
a component — multi-layer "Mikami line search" appears in tools and patents as a
fast candidate generator with a maze fallback behind it. That is precisely the
"Hightower first, grid search on `None`" pattern this crate recommends.

## 5. Neighbouring field: games and robotics

The same "don't touch every cell" instinct developed differently there:

* **Jump Point Search** — lossless A\* speedup on uniform grids; still grid-bound.
* **Theta\*, Block A\***, other any-angle planners — paths that cut through open
  space with few turns.
* **Hierarchical sparse visibility graphs** (e.g. ENLSVG) — optimal any-angle
  paths via precomputed taut-path hierarchies.

None transfers directly to rectilinear layout, but the direction of travel is the
same: sparse graph, not grid; and not greedy line growth.

## 6. Consequences for this crate and the article

1. `blog/hightower.md`, section *Was noch fehlt*, points at Mikami–Tabuchi as the
   route to completeness. Historically right, but the better pointer is
   libavoid / GD 2009: complete, optimal in both length and bends, also grid-free,
   and fifteen years in production.
2. The benchmark compares against a deliberately naive BFS. The fair modern
   opponent is A\* on the orthogonal visibility graph, which is *also* area-
   independent. The article's sentence "a serious A\* would be faster than my
   flood fill, but it would not change the dependency on area" is true for grid
   A\* and false for visibility-graph A\*. This is the one claim in the text a
   domain reader could attack.
3. Possible follow-up work in the crate: an orthogonal-visibility-graph router as
   a second reference implementation next to `hightower::grid::route_grid` — a
   more honest fallback than BFS, and a better benchmark baseline.

## Sources

* [Orthogonal Connector Routing (Wybrow, Marriott, Stuckey, GD 2009)](https://users.monash.edu/~mwybrow/papers/wybrow-gd-2009.pdf)
* [Seeing Around Corners: Fast Orthogonal Connector Routing (Diagrams 2014)](https://users.monash.edu/~mwybrow/papers/marriott-diagrams-2014.pdf)
* [Adaptagrams: libavoid — Overview](https://www.adaptagrams.org/documentation/libavoid.html)
* [Efficient Maze-Running and Line-Search Algorithms for VLSI Layout](http://users.cis.fiu.edu/~iyengar/publication/backup/J-(1993)%20-%20Efficient%20Maze%20Running%20and%20Line%20Search%20Algorithms%20for%20VLSI%20Layout%20-%5BIEEE%5D.pdf)
* [Unit 6: Maze (Area) and Global Routing — Y.-W. Chang, NTU](http://cc.ee.ntu.edu.tw/~ywchang/Courses/EDA/lec6.pdf)
* [Global and Detailed Routing, chapter 12 (Chen / Chang)](https://cc.ee.ntu.edu.tw/~ywchang/Courses/PD_Source/EDA_routing.pdf)
* [Challenges and Approaches in VLSI Routing (ISPD 2022)](https://dl.acm.org/doi/pdf/10.1145/3505170.3511477)
* [TritonRoute-WXL: The Open Source Router](https://vlsicad.ucsd.edu/Publications/Journals/j136.pdf)
* [OpenROAD detailed routing documentation](https://openroad.readthedocs.io/en/latest/main/src/drt/README.html)
* [Accelerating Detailed Routing Convergence through Offline Reinforcement Learning](https://arxiv.org/pdf/2512.03594)
* [XRoute Environment: A Novel Reinforcement Learning Environment for Routing](https://arxiv.org/html/2305.13823)
* [Freerouting](https://www.freerouting.app/)
* [Rectilinear shortest paths through polygonal obstacles in O(n log² n) (Clarkson, Kapoor, Vaidya)](https://dl.acm.org/doi/10.1145/41958.41985)
* [Rectilinear shortest paths in the presence of rectangular obstacles](https://link.springer.com/content/pdf/10.1007/BF02187714.pdf)
* [Edge N-Level Sparse Visibility Graphs (SoCS)](https://ojs.aaai.org/index.php/SOCS/article/download/18427/18218/21943)
* [Any-angle path planning — Wikipedia](https://en.wikipedia.org/wiki/Any-angle_path_planning)
* [A Simple Pipeline for Orthogonal Graph Drawing (GD 2023)](https://arxiv.org/abs/2309.01671)
