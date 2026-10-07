use crate::fonts::FontRegistry;
use crate::model::{Edge, Node, NodeShape, NodeStyle, parallelogram_skew};
use crate::routing::Route;
use egui::epaint::{CornerRadius, PathShape, PathStroke};
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Align, Color32, FontId, Galley, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};
use std::sync::Arc;

pub const SELECT_COLOR: Color32 = Color32::from_rgb(0x2F, 0x80, 0xED);
pub const ARROW_SIZE: f32 = 7.0;

#[derive(Clone, Copy)]
pub struct View {
    pub origin: Pos2,
    pub pan: Vec2,
    pub zoom: f32,
}

impl View {
    pub fn to_screen(&self, p: Pos2) -> Pos2 {
        self.origin + self.pan + p.to_vec2() * self.zoom
    }

    pub fn to_world(&self, s: Pos2) -> Pos2 {
        ((s - self.origin - self.pan) / self.zoom).to_pos2()
    }

    pub fn rect_to_screen(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.to_screen(r.min), self.to_screen(r.max))
    }
}

/// Usable text width inside a node of the given shape.
pub fn text_wrap_width(node: &Node) -> f32 {
    let w = node.size.x;
    let inner = match node.style.shape {
        NodeShape::Diamond => w * 0.62,
        NodeShape::Ellipse => w * 0.78,
        NodeShape::Parallelogram => w - parallelogram_skew(node.rect()) * 1.6,
        NodeShape::Pill => w - node.size.y.min(w) * 0.35,
        NodeShape::Text => w,
        _ => w - 8.0,
    };
    inner.max(10.0)
}

pub fn layout_text(ctx: &egui::Context, fonts: &FontRegistry, text: &str, style: &NodeStyle, wrap: f32, scale: f32) -> Arc<Galley> {
    let font_id = FontId::new((style.font_size * scale).max(1.0), fonts.family(&style.font, style.bold));
    layout_plain(ctx, text, font_id, style.text_color, style.underline, wrap * scale, scale)
}

pub fn layout_plain(ctx: &egui::Context, text: &str, font_id: FontId, color: Color32, underline: bool, wrap: f32, scale: f32) -> Arc<Galley> {
    let mut job = LayoutJob {
        halign: Align::Center,
        wrap: TextWrapping { max_width: wrap, ..Default::default() },
        ..Default::default()
    };
    job.append(
        text,
        0.0,
        TextFormat {
            font_id,
            color,
            underline: if underline { Stroke::new(scale.max(0.5), color) } else { Stroke::NONE },
            ..Default::default()
        },
    );
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// Polygon outline (in the rect's coordinate space) for non-rectangular shapes.
pub fn shape_polygon(rect: Rect, shape: NodeShape) -> Option<Vec<Pos2>> {
    match shape {
        NodeShape::Diamond => Some(vec![
            pos2(rect.center().x, rect.min.y),
            pos2(rect.max.x, rect.center().y),
            pos2(rect.center().x, rect.max.y),
            pos2(rect.min.x, rect.center().y),
        ]),
        NodeShape::Parallelogram => {
            let s = parallelogram_skew(rect);
            Some(vec![
                pos2(rect.min.x + s, rect.min.y),
                pos2(rect.max.x, rect.min.y),
                pos2(rect.max.x - s, rect.max.y),
                pos2(rect.min.x, rect.max.y),
            ])
        }
        NodeShape::Ellipse => {
            let c = rect.center();
            let (a, b) = (rect.width() * 0.5, rect.height() * 0.5);
            Some(
                (0..64)
                    .map(|i| {
                        let t = i as f32 / 64.0 * std::f32::consts::TAU;
                        pos2(c.x + a * t.cos(), c.y + b * t.sin())
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Corner radius in world units for rect-like shapes.
pub fn corner_radius(shape: NodeShape, size: Vec2) -> f32 {
    match shape {
        NodeShape::Rect => 1.5,
        NodeShape::Rounded => 9.0_f32.min(size.min_elem() * 0.5),
        NodeShape::Pill => size.min_elem() * 0.5,
        _ => 0.0,
    }
}

fn paint_body(painter: &Painter, rect: Rect, shape: NodeShape, fill: Color32, stroke: Stroke, zoom: f32) {
    if let Some(points) = shape_polygon(rect, shape) {
        painter.add(Shape::Path(PathShape::convex_polygon(points, fill, PathStroke::from(stroke))));
    } else {
        let r = (corner_radius(shape, rect.size() / zoom) * zoom).round().clamp(0.0, 255.0) as u8;
        painter.rect(rect, CornerRadius::same(r), fill, stroke, StrokeKind::Middle);
    }
}

/// Paints a node. `galley` is the pre-laid-out text (None = don't draw text).
pub fn paint_node(painter: &Painter, view: &View, node: &Node, galley: Option<Arc<Galley>>) {
    let rect = view.rect_to_screen(node.rect());
    let st = &node.style;
    let z = view.zoom;
    if st.shape != NodeShape::Text {
        if st.shadow {
            let off = vec2(2.0, 2.5) * z;
            paint_body(painter, rect.translate(off), st.shape, Color32::from_black_alpha(38), Stroke::NONE, z);
        }
        let stroke = if st.stroke_width > 0.0 { Stroke::new(st.stroke_width * z, st.stroke) } else { Stroke::NONE };
        paint_body(painter, rect, st.shape, st.fill, stroke, z);
    } else if st.fill.a() > 0 {
        painter.rect_filled(rect, CornerRadius::ZERO, st.fill);
    }
    if let Some(g) = galley {
        let pos = rect.center() - g.rect.center().to_vec2();
        painter.galley(pos, g, st.text_color);
    }
}

pub fn paint_selection_outline(painter: &Painter, view: &View, node: &Node, color: Color32, width: f32) {
    let rect = view.rect_to_screen(node.rect()).expand(3.0);
    painter.rect_stroke(rect, CornerRadius::same(3), Stroke::new(width, color), StrokeKind::Middle);
}

pub fn arrow_head(tip: Pos2, dir: Vec2, size: f32) -> Vec<Pos2> {
    let d = dir.normalized();
    let n = vec2(-d.y, d.x);
    let base = tip - d * size * 1.4;
    vec![tip, base + n * size * 0.6, base - n * size * 0.6]
}

pub fn paint_polyline(painter: &Painter, pts: &[Pos2], stroke: Stroke, dashed: bool, arrow: bool, zoom: f32) {
    if pts.len() < 2 {
        return;
    }
    let mut line: Vec<Pos2> = pts.to_vec();
    let n = line.len();
    let dir = line[n - 1] - line[n - 2];
    let tip = line[n - 1];
    let head = ARROW_SIZE * zoom * (stroke.width / zoom / 1.5).max(1.0).sqrt();
    if arrow && dir.length() > 0.1 {
        // Stop the line at the arrow base so the tip stays sharp.
        let back = dir.normalized() * head * 1.2;
        if dir.length() > back.length() {
            line[n - 1] = tip - back;
        }
    }
    if dashed {
        painter.extend(Shape::dashed_line(&line, stroke, 6.0 * zoom, 4.0 * zoom));
    } else {
        painter.add(Shape::line(line, stroke));
    }
    if arrow && dir.length() > 0.1 {
        painter.add(Shape::convex_polygon(arrow_head(tip, dir, head), stroke.color, Stroke::NONE));
    }
}

pub fn paint_edge(painter: &Painter, view: &View, edge: &Edge, route: &Route, highlight: Option<Color32>) {
    let pts: Vec<Pos2> = route.points.iter().map(|p| view.to_screen(*p)).collect();
    let color = highlight.unwrap_or(edge.style.color);
    let width = edge.style.width * view.zoom + if highlight.is_some() { 1.0 } else { 0.0 };
    paint_polyline(painter, &pts, Stroke::new(width, color), edge.style.dashed, edge.style.arrow, view.zoom);
}

pub fn paint_edge_label(painter: &Painter, view: &View, edge: &Edge, anchor: Pos2, ctx: &egui::Context, fonts: &FontRegistry) {
    if edge.label.is_empty() {
        return;
    }
    let font = FontId::new(edge.style.font_size * view.zoom, fonts.family(crate::presets::DEFAULT_FONT, false));
    let g = layout_plain(ctx, &edge.label, font, edge.style.label_color, false, f32::INFINITY, view.zoom);
    let c = view.to_screen(anchor);
    let bg = Rect::from_center_size(c, g.rect.size() + vec2(6.0, 2.0) * view.zoom);
    painter.rect_filled(bg, CornerRadius::same(2), Color32::WHITE);
    painter.galley(c - g.rect.center().to_vec2(), g, edge.style.label_color);
}
