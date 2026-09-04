//! SVG rendering of scenes and traces. No dependencies; the y-axis is flipped
//! so that larger `y` is drawn further up, as in the paper's plots.

use std::fmt::Write;

use crate::geometry::{Bounds, Coord, Orientation, Point, Segment};
use crate::obstacles::ObstacleSet;
use crate::trace::{NetId, Process, Trace, TraceEvent};

/// Colors and stroke widths (in user units before scaling).
#[derive(Clone, Debug)]
#[allow(missing_docs)] // the field names are the documentation
pub struct Style {
    pub background: &'static str,
    pub bounds_stroke: &'static str,
    pub obstacle_stroke: &'static str,
    pub net_a: &'static str,
    pub net_b: &'static str,
    pub intersection: &'static str,
    pub path: &'static str,
    pub raw_path: &'static str,
    pub endpoint_fill: &'static str,
    pub label: &'static str,
    /// Pixels per coordinate unit.
    pub scale: f64,
    /// Padding around the bounds, in coordinate units.
    pub margin: f64,
    /// Stroke width of obstacles in pixels.
    pub obstacle_width: f64,
    pub line_width: f64,
    pub path_width: f64,
    pub dot_radius: f64,
    pub font_size: f64,
    /// Draw the `A` / `B` labels next to the endpoints.
    pub labels: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            background: "#ffffff",
            bounds_stroke: "#bbbbbb",
            obstacle_stroke: "#111111",
            net_a: "#1f6fd6",
            net_b: "#2a9d5c",
            intersection: "#f28c28",
            path: "#d62828",
            raw_path: "#f4a3a3",
            endpoint_fill: "#111111",
            label: "#111111",
            scale: 6.0,
            margin: 3.0,
            obstacle_width: 3.0,
            line_width: 1.4,
            path_width: 3.5,
            dot_radius: 3.0,
            font_size: 14.0,
            labels: true,
        }
    }
}

impl Style {
    /// Scale chosen so that the bounds fit into roughly `target_px` pixels on their longer side.
    pub fn fit(bounds: Bounds, target_px: f64) -> Self {
        let longer = bounds.width().max(bounds.height()).max(1) as f64;
        Style {
            scale: target_px / longer,
            ..Style::default()
        }
    }

    fn color(&self, net: NetId) -> &'static str {
        match net {
            NetId::A => self.net_a,
            NetId::B => self.net_b,
        }
    }
}

/// Which layers of a trace to draw.
#[derive(Clone, Copy, Debug)]
#[allow(missing_docs)]
pub struct Layers {
    pub lines: bool,
    pub probes: bool,
    pub escape_points: bool,
    pub intersection: bool,
    pub raw_path: bool,
    pub final_path: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Layers {
            lines: true,
            probes: true,
            escape_points: true,
            intersection: true,
            raw_path: false,
            final_path: true,
        }
    }
}

/// A scene: obstacles plus the two endpoints.
#[derive(Clone, Debug)]
pub struct Scene<'a> {
    /// The obstacles and bounds.
    pub obstacles: &'a ObstacleSet,
    /// Start point.
    pub a: Point,
    /// Target point.
    pub b: Point,
}

/// Incrementally builds an SVG document. Use this for custom figures; the
/// [`render`] family covers the common cases.
pub struct Canvas {
    style: Style,
    bounds: Bounds,
    body: String,
}

impl Canvas {
    /// A canvas covering `bounds` (plus margin).
    pub fn new(bounds: Bounds, style: Style) -> Self {
        Canvas {
            style,
            bounds,
            body: String::new(),
        }
    }

    fn px(&self, p: Point) -> (f64, f64) {
        let s = self.style.scale;
        let x = (p.x - self.bounds.min.x) as f64 + self.style.margin;
        let y = (self.bounds.max.y - p.y) as f64 + self.style.margin;
        (x * s, y * s)
    }

    /// Pixel size of the whole image.
    pub fn size(&self) -> (f64, f64) {
        let s = self.style.scale;
        let m = 2.0 * self.style.margin;
        (
            (self.bounds.width() as f64 + m) * s,
            (self.bounds.height() as f64 + m) * s,
        )
    }

    /// Appends raw SVG markup.
    pub fn raw(&mut self, markup: &str) {
        self.body.push_str(markup);
        self.body.push('\n');
    }

    /// Draws a segment.
    pub fn segment(&mut self, s: &Segment, stroke: &str, width: f64, extra: &str) {
        let (x1, y1) = self.px(s.start());
        let (x2, y2) = self.px(s.end());
        let _ = writeln!(
            self.body,
            r#"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="{stroke}" stroke-width="{width}" stroke-linecap="round" {extra}/>"#
        );
    }

    /// Draws a polyline through the corners.
    pub fn polyline(&mut self, corners: &[Point], stroke: &str, width: f64, extra: &str) {
        if corners.len() == 1 {
            self.dot(corners[0], stroke, width);
            return;
        }
        let pts: Vec<String> = corners
            .iter()
            .map(|&p| {
                let (x, y) = self.px(p);
                format!("{x:.1},{y:.1}")
            })
            .collect();
        let _ = writeln!(
            self.body,
            r#"<polyline points="{}" fill="none" stroke="{stroke}" stroke-width="{width}" stroke-linejoin="round" stroke-linecap="round" {extra}/>"#,
            pts.join(" ")
        );
    }

    /// Draws a filled dot.
    pub fn dot(&mut self, p: Point, fill: &str, r: f64) {
        let (x, y) = self.px(p);
        let _ = writeln!(
            self.body,
            r#"<circle cx="{x:.1}" cy="{y:.1}" r="{r}" fill="{fill}"/>"#
        );
    }

    /// Draws a hollow circle.
    pub fn ring(&mut self, p: Point, stroke: &str, r: f64, width: f64) {
        let (x, y) = self.px(p);
        let _ = writeln!(
            self.body,
            r#"<circle cx="{x:.1}" cy="{y:.1}" r="{r}" fill="none" stroke="{stroke}" stroke-width="{width}"/>"#
        );
    }

    /// Draws a filled unit cell centred on a lattice point.
    pub fn cell(&mut self, p: Point, fill: &str, opacity: f64) {
        let (x, y) = self.px(p);
        let s = self.style.scale;
        let _ = writeln!(
            self.body,
            r#"<rect x="{:.1}" y="{:.1}" width="{s:.2}" height="{s:.2}" fill="{fill}" fill-opacity="{opacity:.2}"/>"#,
            x - s / 2.0,
            y - s / 2.0
        );
    }

    /// Draws a text label with its anchor offset in pixels.
    pub fn text(&mut self, p: Point, dx: f64, dy: f64, label: &str, fill: &str) {
        let (x, y) = self.px(p);
        let fs = self.style.font_size;
        let _ = writeln!(
            self.body,
            r#"<text x="{:.1}" y="{:.1}" font-family="Helvetica, Arial, sans-serif" font-size="{fs}" fill="{fill}">{}</text>"#,
            x + dx,
            y + dy,
            escape_xml(label)
        );
    }

    /// Draws the bounding frame.
    pub fn frame(&mut self) {
        let (x1, y1) = self.px(Point::new(self.bounds.min.x, self.bounds.max.y));
        let (x2, y2) = self.px(Point::new(self.bounds.max.x, self.bounds.min.y));
        let _ = writeln!(
            self.body,
            r#"<rect x="{x1:.1}" y="{y1:.1}" width="{:.1}" height="{:.1}" fill="none" stroke="{}" stroke-width="1"/>"#,
            x2 - x1,
            y2 - y1,
            self.style.bounds_stroke
        );
    }

    /// Draws all obstacles.
    pub fn obstacles(&mut self, obstacles: &ObstacleSet) {
        let (stroke, width) = (self.style.obstacle_stroke, self.style.obstacle_width);
        for s in obstacles.segments() {
            self.segment(&s, stroke, width, "");
        }
    }

    /// Draws the two endpoints (and labels, if enabled).
    pub fn endpoints(&mut self, a: Point, b: Point) {
        let r = self.style.dot_radius + 1.5;
        let fill = self.style.endpoint_fill;
        self.dot(a, fill, r);
        self.dot(b, fill, r);
        if self.style.labels {
            let label = self.style.label;
            self.text(a, -r - 12.0, -r - 3.0, "A", label);
            self.text(b, r + 4.0, -r - 3.0, "B", label);
        }
    }

    /// Draws the first `upto` events of a trace.
    pub fn trace(&mut self, trace: &Trace, upto: usize, layers: Layers) {
        let events = &trace.events[..upto.min(trace.events.len())];
        let style = self.style.clone();
        if layers.probes {
            for e in events {
                if let TraceEvent::ProbeLine { net, line, .. } = e {
                    self.segment(
                        line,
                        style.color(*net),
                        style.line_width * 0.7,
                        r#"stroke-opacity="0.35" stroke-dasharray="3 3""#,
                    );
                }
            }
        }
        if layers.lines {
            for e in events {
                if let TraceEvent::LineAdded { net, line, .. } = e {
                    self.segment(
                        line,
                        style.color(*net),
                        style.line_width,
                        r#"stroke-opacity="0.85""#,
                    );
                }
            }
        }
        if layers.escape_points {
            for e in events {
                if let TraceEvent::EscapePoint {
                    net,
                    point,
                    process,
                } = e
                {
                    let r = match process {
                        Process::I => style.dot_radius,
                        Process::II => style.dot_radius * 0.8,
                    };
                    self.dot(*point, style.color(*net), r);
                }
            }
        }
        if layers.raw_path {
            for e in events {
                if let TraceEvent::RawPath { corners } = e {
                    self.polyline(
                        corners,
                        style.raw_path,
                        style.path_width,
                        r#"stroke-dasharray="6 4""#,
                    );
                }
            }
        }
        if layers.final_path {
            let last = events.iter().rev().find_map(|e| match e {
                TraceEvent::RawPath { corners } | TraceEvent::Improved { corners } => Some(corners),
                _ => None,
            });
            if let Some(corners) = last {
                self.polyline(corners, style.path, style.path_width, "");
            }
        }
        if layers.intersection {
            for e in events {
                if let TraceEvent::Intersection { point, .. } = e {
                    self.ring(*point, style.intersection, style.dot_radius * 2.2, 2.5);
                }
            }
        }
    }

    /// Draws a segment of the given orientation through `p`, dashed, to mark a cover or an escape line.
    pub fn marker_segment(&mut self, s: &Segment, stroke: &str, width: f64) {
        self.segment(s, stroke, width, r#"stroke-dasharray="5 3""#);
    }

    /// Finishes the document.
    pub fn finish(self) -> String {
        let (w, h) = self.size();
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\n<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>\n{}</svg>\n",
            self.style.background, self.body
        )
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Renders the scene and the first `upto` trace events.
pub fn render(scene: &Scene, trace: &Trace, upto: usize, style: &Style, layers: Layers) -> String {
    let mut c = Canvas::new(scene.obstacles.bounds(), style.clone());
    c.frame();
    c.obstacles(scene.obstacles);
    c.trace(trace, upto, layers);
    c.endpoints(scene.a, scene.b);
    c.finish()
}

/// Renders the scene with the complete trace and default layers.
pub fn render_final(scene: &Scene, trace: &Trace, style: &Style) -> String {
    render(scene, trace, trace.len(), style, Layers::default())
}

/// Renders the scene with only the final path (no construction lines).
pub fn render_path(scene: &Scene, path: &[Point], style: &Style) -> String {
    let mut c = Canvas::new(scene.obstacles.bounds(), style.clone());
    c.frame();
    c.obstacles(scene.obstacles);
    c.polyline(path, style.path, style.path_width, "");
    c.endpoints(scene.a, scene.b);
    c.finish()
}

/// Renders one point's four covers and two escape lines (blog figure helper).
/// Highlights the covers of `p` (and optionally its escape lines). `p` is
/// usually one of the endpoints, which are drawn with their labels; any other
/// point is labelled `p`.
pub fn render_covers(scene: &Scene, p: Point, style: &Style, show_escape_lines: bool) -> String {
    let obstacles = scene.obstacles;
    let mut c = Canvas::new(obstacles.bounds(), style.clone());
    c.frame();
    // dim all obstacles, then highlight the covers
    for s in obstacles.segments() {
        c.segment(&s, "#c8c8c8", style.obstacle_width, "");
    }
    let covers = [
        obstacles.cover_above(p),
        obstacles.cover_below(p),
        obstacles.cover_left(p),
        obstacles.cover_right(p),
    ];
    for cover in covers.into_iter().flatten() {
        c.segment(&cover, style.obstacle_stroke, style.obstacle_width, "");
    }
    if show_escape_lines {
        let h = obstacles.escape_line(p, Orientation::Horizontal);
        let v = obstacles.escape_line(p, Orientation::Vertical);
        c.segment(&h, style.net_a, style.line_width * 1.6, "");
        c.segment(&v, style.net_a, style.line_width * 1.6, "");
    }
    c.endpoints(scene.a, scene.b);
    if p != scene.a && p != scene.b {
        c.dot(p, style.endpoint_fill, style.dot_radius + 1.5);
        c.text(p, 8.0, 16.0, "p", style.label);
    }
    c.finish()
}

/// Renders a Lee-style flood fill: visited cells shaded by BFS distance, then the path.
pub fn render_flood(
    scene: &Scene,
    visited: &[(Point, u32)],
    path: Option<&[Point]>,
    style: &Style,
) -> String {
    let mut c = Canvas::new(scene.obstacles.bounds(), style.clone());
    let max_d = visited.iter().map(|&(_, d)| d).max().unwrap_or(1).max(1) as f64;
    for &(p, d) in visited {
        let t = d as f64 / max_d;
        // light blue near the start, deep blue far away
        let (r, g, b) = (
            (225.0 - 170.0 * t) as u8,
            (236.0 - 120.0 * t) as u8,
            (250.0 - 60.0 * t) as u8,
        );
        c.cell(p, &format!("#{r:02x}{g:02x}{b:02x}"), 1.0);
    }
    c.frame();
    c.obstacles(scene.obstacles);
    if let Some(corners) = path {
        c.polyline(corners, style.path, style.path_width, "");
    }
    c.endpoints(scene.a, scene.b);
    c.finish()
}

/// Convenience: a rectangle helper for figure code.
pub fn rect_segments(min: Point, max: Point) -> [Segment; 4] {
    [
        Segment::horizontal(min.y, min.x, max.x),
        Segment::horizontal(max.y, min.x, max.x),
        Segment::vertical(min.x, min.y, max.y),
        Segment::vertical(max.x, min.y, max.y),
    ]
}

/// Convenience: a coordinate cast for figure code.
pub fn c(v: i32) -> Coord {
    v as Coord
}
