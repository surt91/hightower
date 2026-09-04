//! The editor: a square board of `SIZE` x `SIZE` units with axis-parallel
//! boxes and the two endpoints A and B. Every frame the scene is handed to
//! the library and the resulting networks and path are drawn.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use hightower::{
    Bounds, Improvement, NetId, ObstacleSet, Outcome, Point, RouterConfig, Segment, TraceEvent,
    route_visibility, route_with,
};

/// Side length of the board in routing units (the article's running scene).
const SIZE: i64 = 100;

// The palette of the article figures.
const OBSTACLE: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
const FRAME: Color32 = Color32::from_rgb(0xbb, 0xbb, 0xbb);
const NET_A: Color32 = Color32::from_rgb(0x1f, 0x6f, 0xd6);
const NET_B: Color32 = Color32::from_rgb(0x2a, 0x9d, 0x5c);
const INTERSECTION: Color32 = Color32::from_rgb(0xf2, 0x8c, 0x28);
const PATH: Color32 = Color32::from_rgb(0xd6, 0x28, 0x28);
const SHORTEST: Color32 = Color32::from_rgb(0x99, 0x99, 0x99);
const HIGHLIGHT: Color32 = Color32::from_rgba_premultiplied(0x1f, 0x6f, 0xd6, 0x18);

/// Pointer distance (in points) within which a handle counts as grabbed.
const GRAB_RADIUS: f32 = 9.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Block {
    min: Point,
    max: Point,
}

impl Block {
    fn new(a: Point, b: Point) -> Self {
        Self {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    fn corners(&self) -> [Point; 4] {
        [
            self.min,
            Point::new(self.max.x, self.min.y),
            self.max,
            Point::new(self.min.x, self.max.y),
        ]
    }

    /// The corner diagonally opposite to `corner`.
    fn opposite(&self, corner: Point) -> Point {
        Point::new(
            if corner.x == self.min.x {
                self.max.x
            } else {
                self.min.x
            },
            if corner.y == self.min.y {
                self.max.y
            } else {
                self.min.y
            },
        )
    }

    fn contains(&self, p: Point) -> bool {
        self.min.x <= p.x && p.x <= self.max.x && self.min.y <= p.y && p.y <= self.max.y
    }

    fn is_degenerate(&self) -> bool {
        self.min.x == self.max.x || self.min.y == self.max.y
    }

    fn shifted(&self, dx: i64, dy: i64) -> Self {
        Self {
            min: Point::new(self.min.x + dx, self.min.y + dy),
            max: Point::new(self.max.x + dx, self.max.y + dy),
        }
    }
}

/// What the pointer is currently dragging.
#[derive(Clone, Copy, Debug)]
enum Drag {
    EndpointA,
    EndpointB,
    /// Moving a whole block; `grab` is the pointer offset from its `min`.
    Move {
        index: usize,
        grab: Point,
    },
    /// Dragging one corner, the opposite corner stays put.
    Resize {
        index: usize,
        anchor: Point,
    },
    /// Rubber band for a new block.
    Create {
        start: Point,
        current: Point,
    },
}

/// What the pointer is hovering, decides the cursor and the highlight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hover {
    EndpointA,
    EndpointB,
    Corner(usize),
    Block(usize),
    Empty,
}

/// Maps board units to screen points. The board keeps its aspect ratio and
/// uses y pointing up, like the SVG figures.
struct View {
    origin: Pos2,
    scale: f32,
}

impl View {
    fn fit(rect: Rect) -> Self {
        let side = rect.width().min(rect.height()) - 2.0 * 16.0;
        let scale = side / SIZE as f32;
        let origin = Pos2::new(rect.center().x - side / 2.0, rect.center().y + side / 2.0);
        Self { origin, scale }
    }

    fn to_screen(&self, p: Point) -> Pos2 {
        Pos2::new(
            self.origin.x + p.x as f32 * self.scale,
            self.origin.y - p.y as f32 * self.scale,
        )
    }

    fn to_board(&self, pos: Pos2) -> Point {
        let x = ((pos.x - self.origin.x) / self.scale).round() as i64;
        let y = ((self.origin.y - pos.y) / self.scale).round() as i64;
        Point::new(x.clamp(0, SIZE), y.clamp(0, SIZE))
    }

    fn board_rect(&self) -> Rect {
        Rect::from_two_pos(
            self.to_screen(Point::new(0, 0)),
            self.to_screen(Point::new(SIZE, SIZE)),
        )
    }

    fn block_rect(&self, b: &Block) -> Rect {
        Rect::from_two_pos(self.to_screen(b.min), self.to_screen(b.max))
    }
}

pub struct App {
    blocks: Vec<Block>,
    a: Point,
    b: Point,
    drag: Option<Drag>,
    show_lines: bool,
    show_probes: bool,
    show_shortest: bool,
    improve: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Light);
        let mut app = Self {
            blocks: Vec::new(),
            a: Point::new(0, 0),
            b: Point::new(0, 0),
            drag: None,
            show_lines: true,
            show_probes: false,
            show_shortest: false,
            improve: true,
        };
        app.reset_scene();
        app
    }

    /// The running example of the article: eight boxes on a 100 x 100 board.
    fn reset_scene(&mut self) {
        self.blocks = [
            (5, 2, 34, 19),
            (13, 12, 42, 23),
            (41, 33, 55, 56),
            (30, 47, 62, 67),
            (78, 53, 87, 75),
            (72, 56, 98, 76),
            (1, 76, 31, 97),
            (59, 84, 70, 96),
        ]
        .into_iter()
        .map(|(x1, y1, x2, y2)| Block::new(Point::new(x1, y1), Point::new(x2, y2)))
        .collect();
        self.a = Point::new(38, 46);
        self.b = Point::new(63, 20);
        self.drag = None;
    }

    fn obstacles(&self) -> ObstacleSet {
        let mut o = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(SIZE, SIZE)));
        for b in &self.blocks {
            o.add_rect(b.min, b.max);
        }
        o
    }

    fn hover_at(&self, view: &View, pos: Pos2) -> Hover {
        let near = |p: Point| view.to_screen(p).distance(pos) <= GRAB_RADIUS;
        if near(self.a) {
            return Hover::EndpointA;
        }
        if near(self.b) {
            return Hover::EndpointB;
        }
        // Corners first (small blocks would otherwise be impossible to resize),
        // topmost block wins.
        for (i, b) in self.blocks.iter().enumerate().rev() {
            if b.corners().into_iter().any(near) {
                return Hover::Corner(i);
            }
        }
        let board = view.to_board(pos);
        for (i, b) in self.blocks.iter().enumerate().rev() {
            if view.block_rect(b).contains(pos) && b.contains(board) {
                return Hover::Block(i);
            }
        }
        Hover::Empty
    }

    fn begin_drag(&mut self, view: &View, pos: Pos2) {
        let board = view.to_board(pos);
        self.drag = Some(match self.hover_at(view, pos) {
            Hover::EndpointA => Drag::EndpointA,
            Hover::EndpointB => Drag::EndpointB,
            Hover::Corner(index) => {
                let block = self.blocks[index];
                let corner = block
                    .corners()
                    .into_iter()
                    .min_by_key(|c| c.dist2(board))
                    .expect("four corners");
                Drag::Resize {
                    index,
                    anchor: block.opposite(corner),
                }
            }
            Hover::Block(index) => Drag::Move {
                index,
                grab: Point::new(
                    board.x - self.blocks[index].min.x,
                    board.y - self.blocks[index].min.y,
                ),
            },
            Hover::Empty => Drag::Create {
                start: board,
                current: board,
            },
        });
    }

    fn update_drag(&mut self, view: &View, pos: Pos2) {
        let board = view.to_board(pos);
        match self.drag {
            Some(Drag::EndpointA) => self.a = board,
            Some(Drag::EndpointB) => self.b = board,
            Some(Drag::Move { index, grab }) => {
                let block = self.blocks[index];
                let (w, h) = (block.max.x - block.min.x, block.max.y - block.min.y);
                let min_x = (board.x - grab.x).clamp(0, SIZE - w);
                let min_y = (board.y - grab.y).clamp(0, SIZE - h);
                self.blocks[index] = block.shifted(min_x - block.min.x, min_y - block.min.y);
            }
            Some(Drag::Resize { index, anchor }) => {
                self.blocks[index] = Block::new(anchor, board);
            }
            Some(Drag::Create { start, .. }) => {
                self.drag = Some(Drag::Create {
                    start,
                    current: board,
                });
            }
            None => {}
        }
    }

    fn end_drag(&mut self) {
        match self.drag.take() {
            Some(Drag::Create { start, current }) => {
                let block = Block::new(start, current);
                if !block.is_degenerate() {
                    self.blocks.push(block);
                }
            }
            Some(Drag::Resize { index, .. }) if self.blocks[index].is_degenerate() => {
                self.blocks.remove(index);
            }
            _ => {}
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui, stats: &Stats) {
        ui.heading("Hightower line router");
        ui.label("Drag boxes and the endpoints A and B. Drag on empty ground to draw a new box, drag a corner to resize it, right-click a box to delete it.");
        ui.add_space(8.0);

        ui.checkbox(&mut self.show_lines, "Escape lines of both networks");
        ui.checkbox(&mut self.show_probes, "Trial lines of Process II (dashed)");
        ui.checkbox(&mut self.improve, "Second improvement (Figure 12)");
        ui.checkbox(
            &mut self.show_shortest,
            "Shortest path for comparison (gray)",
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Reset to article scene").clicked() {
                self.reset_scene();
            }
            if ui.button("Remove all boxes").clicked() {
                self.blocks.clear();
            }
        });
        ui.add_space(12.0);

        egui::Grid::new("stats")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label("Outcome");
                ui.label(stats.outcome);
                ui.end_row();
                ui.label("Steps");
                ui.label(stats.steps.to_string());
                ui.end_row();
                ui.label("Lines entered");
                ui.label(stats.lines.to_string());
                ui.end_row();
                ui.label("Trial lines");
                ui.label(stats.probes.to_string());
                ui.end_row();
                ui.label("Path length");
                ui.label(stats.path_length.map_or("-".to_string(), |l| l.to_string()));
                ui.end_row();
                ui.label("Corners");
                ui.label(stats.corners.map_or("-".to_string(), |c| c.to_string()));
                ui.end_row();
                ui.label("Shortest length");
                ui.label(
                    stats
                        .shortest_length
                        .map_or("-".to_string(), |l| l.to_string()),
                );
                ui.end_row();
            });
        if let Some(hint) = stats.hint {
            ui.add_space(8.0);
            ui.colored_label(PATH, hint);
        }
        ui.add_space(12.0);
        ui.small(
            "Blue: network of A. Green: network of B. Orange: the intersection. Red: the path.",
        );
        ui.small("D. W. Hightower, A Solution to Line-Routing Problems on the Continuous Plane, DAC 1969.");
    }
}

/// Numbers shown in the side panel.
struct Stats {
    outcome: &'static str,
    steps: usize,
    lines: usize,
    probes: usize,
    path_length: Option<i64>,
    corners: Option<usize>,
    shortest_length: Option<i64>,
    hint: Option<&'static str>,
}

fn length(path: &[Point]) -> i64 {
    path.windows(2).map(|w| w[0].manhattan(w[1])).sum()
}

fn segment_points(view: &View, s: &Segment) -> [Pos2; 2] {
    [view.to_screen(s.start()), view.to_screen(s.end())]
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let obstacles = self.obstacles();
        let config = RouterConfig {
            improve: if self.improve {
                Improvement::Full
            } else {
                Improvement::None
            },
            ..RouterConfig::default()
        };
        let result = route_with(&obstacles, self.a, self.b, &config);
        let shortest = route_visibility(&obstacles, self.a, self.b);
        let endpoints_free = obstacles.is_free_point(self.a) && obstacles.is_free_point(self.b);
        let stats = Stats {
            outcome: match result.outcome {
                Outcome::Found => "path found",
                Outcome::NoEscape => "no escape (both networks stuck)",
                Outcome::StepLimit => "step limit reached",
                Outcome::InvalidInput => "invalid input",
            },
            steps: result.steps,
            lines: result.trace.line_count(),
            probes: result.trace.probe_count(),
            path_length: result.path.as_deref().map(length),
            corners: result.path.as_ref().map(Vec::len),
            shortest_length: shortest.as_deref().map(length),
            hint: if !endpoints_free {
                Some("A or B touches an obstacle; move it to free ground.")
            } else if result.outcome == Outcome::NoEscape && shortest.is_some() {
                Some(
                    "A path exists, but Hightower's used-line rule blocks it (see the article's blind spot).",
                )
            } else {
                None
            },
        };

        egui::Panel::right("controls")
            .exact_size(300.0)
            .show(ui, |ui| self.controls(ui, &stats));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::WHITE))
            .show(ui, |ui| {
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
                let view = View::fit(response.rect);

                // --- interaction -------------------------------------------------
                let hover = response
                    .hover_pos()
                    .map(|pos| self.hover_at(&view, pos))
                    .unwrap_or(Hover::Empty);
                if let Some(pos) = response.interact_pointer_pos() {
                    if response.drag_started() {
                        self.begin_drag(&view, pos);
                    }
                    if response.dragged() {
                        self.update_drag(&view, pos);
                    }
                }
                if response.drag_stopped() {
                    self.end_drag();
                }
                if response.secondary_clicked()
                    || ctx.input(|i| {
                        i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                    })
                {
                    match hover {
                        Hover::Block(i) | Hover::Corner(i) if self.drag.is_none() => {
                            self.blocks.remove(i);
                        }
                        _ => {}
                    }
                }
                if response.hovered() {
                    ctx.set_cursor_icon(match (self.drag, hover) {
                        (Some(Drag::Create { .. }), _) | (None, Hover::Empty) => {
                            egui::CursorIcon::Crosshair
                        }
                        (Some(Drag::Resize { .. }), _) | (None, Hover::Corner(_)) => {
                            egui::CursorIcon::ResizeNwSe
                        }
                        (Some(_), _) => egui::CursorIcon::Grabbing,
                        (None, _) => egui::CursorIcon::Grab,
                    });
                }

                // --- drawing -------------------------------------------------------
                painter.rect_stroke(
                    view.board_rect(),
                    CornerRadius::ZERO,
                    Stroke::new(1.0, FRAME),
                    StrokeKind::Middle,
                );

                let obstacle_stroke = Stroke::new(2.0, OBSTACLE);
                for (i, block) in self.blocks.iter().enumerate() {
                    let rect = view.block_rect(block);
                    if matches!(hover, Hover::Block(j) | Hover::Corner(j) if j == i) {
                        painter.rect_filled(rect, CornerRadius::ZERO, HIGHLIGHT);
                    }
                    painter.rect_stroke(
                        rect,
                        CornerRadius::ZERO,
                        obstacle_stroke,
                        StrokeKind::Middle,
                    );
                }
                if let Some(Drag::Create { start, current }) = self.drag {
                    let rect = Rect::from_two_pos(view.to_screen(start), view.to_screen(current));
                    painter.rect_stroke(
                        rect,
                        CornerRadius::ZERO,
                        Stroke::new(1.5, NET_A),
                        StrokeKind::Middle,
                    );
                }

                let net_color = |net: NetId| if net == NetId::A { NET_A } else { NET_B };
                for event in &result.trace.events {
                    match event {
                        TraceEvent::LineAdded { net, line, .. } if self.show_lines => {
                            painter.line_segment(
                                segment_points(&view, line),
                                Stroke::new(1.2, net_color(*net)),
                            );
                        }
                        TraceEvent::ProbeLine { net, line, .. } if self.show_probes => {
                            let [p, q] = segment_points(&view, line);
                            painter.extend(egui::Shape::dashed_line(
                                &[p, q],
                                Stroke::new(1.0, net_color(*net).gamma_multiply(0.6)),
                                4.0,
                                4.0,
                            ));
                        }
                        TraceEvent::EscapePoint { net, point, .. } if self.show_lines => {
                            painter.circle_filled(view.to_screen(*point), 3.0, net_color(*net));
                        }
                        TraceEvent::Intersection { point, .. } if self.show_lines => {
                            painter.circle_stroke(
                                view.to_screen(*point),
                                6.0,
                                Stroke::new(2.0, INTERSECTION),
                            );
                        }
                        _ => {}
                    }
                }

                if let (true, Some(path)) = (self.show_shortest, &shortest) {
                    let pts: Vec<Pos2> = path.iter().map(|&p| view.to_screen(p)).collect();
                    painter.extend(egui::Shape::dashed_line(
                        &pts,
                        Stroke::new(2.0, SHORTEST),
                        6.0,
                        4.0,
                    ));
                }
                if let Some(path) = &result.path {
                    for w in path.windows(2) {
                        painter.line_segment(
                            [view.to_screen(w[0]), view.to_screen(w[1])],
                            Stroke::new(3.0, PATH),
                        );
                    }
                    for &corner in path {
                        painter.circle_filled(view.to_screen(corner), 1.5, PATH);
                    }
                }

                let font = FontId::proportional(15.0);
                for (p, label, align, offset) in [
                    (self.a, "A", Align2::RIGHT_BOTTOM, Vec2::new(-6.0, -4.0)),
                    (self.b, "B", Align2::LEFT_BOTTOM, Vec2::new(6.0, -4.0)),
                ] {
                    let pos = view.to_screen(p);
                    painter.circle_filled(pos, 5.0, OBSTACLE);
                    painter.text(pos + offset, align, label, font.clone(), OBSTACLE);
                }
            });
    }
}
