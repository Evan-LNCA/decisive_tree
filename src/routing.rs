//! Orthogonal (right-angle) connector routing.
//!
//! Fan-out (one source side feeding many targets) and fan-in (many sources
//! feeding one target side) share a common trunk close to the shared node,
//! producing the "bus" look of hand-drawn process maps.

use crate::model::{Doc, Edge, LabelPos, Node, Side};
use egui::{Pos2, Rect, Vec2, pos2};
use std::collections::HashMap;

pub const STUB: f32 = 18.0;
const FAN_TRUNK: f32 = 22.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Axis {
    /// Segment is vertical; dragging moves it along X.
    X,
    /// Segment is horizontal; dragging moves it along Y.
    Y,
}

impl Axis {
    fn coordinate(self, p: Pos2) -> f32 {
        match self {
            Self::X => p.x,
            Self::Y => p.y,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Route {
    pub points: Vec<Pos2>,
    pub from_side: Side,
    pub to_side: Side,
    /// Position of the draggable middle segment and the axis it moves along.
    pub handle: Option<(Pos2, Axis)>,
}

pub fn compute_routes(doc: &Doc) -> HashMap<u64, Route> {
    let nodes: HashMap<u64, &Node> = doc.nodes.iter().map(|n| (n.id, n)).collect();
    let mut resolved: Vec<(&Edge, &Node, &Node, Side, Side)> = Vec::with_capacity(doc.edges.len());
    let mut out_count: HashMap<(u64, Side), u32> = HashMap::new();
    let mut in_count: HashMap<(u64, Side), u32> = HashMap::new();

    for e in &doc.edges {
        let (Some(a), Some(b)) = (nodes.get(&e.from), nodes.get(&e.to)) else { continue };
        let (s1, s2) = resolve_sides(e.from_side, e.to_side, a.rect(), b.rect());
        *out_count.entry((a.id, s1)).or_default() += 1;
        *in_count.entry((b.id, s2)).or_default() += 1;
        resolved.push((e, a, b, s1, s2));
    }

    let mut routes = HashMap::with_capacity(resolved.len());
    for (e, a, b, s1, s2) in resolved {
        let fan_src = out_count.get(&(a.id, s1)).copied().unwrap_or(0) > 1;
        let fan_dst = in_count.get(&(b.id, s2)).copied().unwrap_or(0) > 1;
        let input = RouteInput {
            p1: a.port(s1),
            s1,
            p2: b.port(s2),
            s2,
            r1: a.rect(),
            r2: b.rect(),
            fan_src,
            fan_dst,
            bend: e.bend,
        };
        let (points, handle) = route(&input);
        routes.insert(e.id, Route { points: simplify(points), from_side: s1, to_side: s2, handle });
    }
    routes
}

/// Aligns a dragged bend with parallel connector segments or node centerlines.
pub fn snap_bend(doc: &Doc, routes: &HashMap<u64, Route>, edge: u64, bend: f32, tolerance: f32) -> Option<(f32, (Pos2, Pos2))> {
    let current = doc.edge(edge)?;
    let (handle, axis) = routes.get(&edge)?.handle?;
    let base = axis.coordinate(handle) - current.bend;
    let desired = base + bend;
    let mut best: Option<(f32, f32, (Pos2, Pos2))> = None;
    let mut consider = |a: Pos2, b: Pos2| {
        let coordinate = axis.coordinate(a);
        let distance = (coordinate - desired).abs();
        if distance <= tolerance && best.is_none_or(|best| distance < best.0) {
            let guide = match axis {
                Axis::X => (pos2(coordinate, a.y.min(b.y).min(handle.y)), pos2(coordinate, a.y.max(b.y).max(handle.y))),
                Axis::Y => (pos2(a.x.min(b.x).min(handle.x), coordinate), pos2(a.x.max(b.x).max(handle.x), coordinate)),
            };
            best = Some((distance, coordinate - base, guide));
        }
    };
    for n in &doc.nodes {
        let r = n.rect();
        match axis {
            Axis::X => consider(pos2(r.center().x, r.min.y), pos2(r.center().x, r.max.y)),
            Axis::Y => consider(pos2(r.min.x, r.center().y), pos2(r.max.x, r.center().y)),
        }
    }
    for e in doc.edges.iter().filter(|e| e.id != edge) {
        if let Some(route) = routes.get(&e.id) {
            for segment in route.points.windows(2) {
                let (a, b) = (segment[0], segment[1]);
                if (axis.coordinate(a) - axis.coordinate(b)).abs() < 0.01 && (b - a).length_sq() > 0.01 {
                    consider(a, b);
                }
            }
        }
    }
    best.map(|(_, bend, guide)| (bend, guide))
}

/// Route for an in-progress connection drag (target is a free point).
pub fn preview_route(from: &Node, from_side: Option<Side>, target: Pos2, target_node: Option<(&Node, Option<Side>)>) -> Vec<Pos2> {
    let target_rect = match target_node {
        Some((n, _)) => n.rect(),
        None => Rect::from_center_size(target, Vec2::splat(1.0)),
    };
    let (s1, s2) = resolve_sides(from_side, target_node.and_then(|(_, s)| s), from.rect(), target_rect);
    let p2 = match target_node {
        Some((n, _)) => n.port(s2),
        None => target,
    };
    let input = RouteInput {
        p1: from.port(s1),
        s1,
        p2,
        s2,
        r1: from.rect(),
        r2: target_rect,
        fan_src: false,
        fan_dst: false,
        bend: 0.0,
    };
    simplify(route(&input).0)
}

pub fn resolve_sides(from: Option<Side>, to: Option<Side>, a: Rect, b: Rect) -> (Side, Side) {
    match (from, to) {
        (Some(f), Some(t)) => (f, t),
        (Some(f), None) => (f, auto_end_side(f, a, b)),
        (None, Some(t)) => (auto_end_side(t, b, a), t),
        (None, None) => auto_sides(a, b),
    }
}

fn auto_sides(a: Rect, b: Rect) -> (Side, Side) {
    let gx = (b.min.x - a.max.x).max(a.min.x - b.max.x);
    let gy = (b.min.y - a.max.y).max(a.min.y - b.max.y);
    let horizontal = if gx > 0.0 && gy > 0.0 {
        gx >= gy * 0.6
    } else if gx > 0.0 {
        true
    } else if gy > 0.0 {
        false
    } else {
        let d = b.center() - a.center();
        d.x.abs() >= d.y.abs()
    };
    let d = b.center() - a.center();
    if horizontal {
        if d.x >= 0.0 { (Side::Right, Side::Left) } else { (Side::Left, Side::Right) }
    } else if d.y >= 0.0 {
        (Side::Bottom, Side::Top)
    } else {
        (Side::Top, Side::Bottom)
    }
}

/// Given a fixed side on node `a`, pick the best side on node `b`.
fn auto_end_side(fixed: Side, a: Rect, b: Rect) -> Side {
    let d = b.center() - a.center();
    match fixed {
        Side::Right if b.min.x > a.max.x + STUB => Side::Left,
        Side::Left if b.max.x < a.min.x - STUB => Side::Right,
        Side::Bottom if b.min.y > a.max.y + STUB => Side::Top,
        Side::Top if b.max.y < a.min.y - STUB => Side::Bottom,
        Side::Left | Side::Right => {
            if d.y >= 0.0 { Side::Top } else { Side::Bottom }
        }
        Side::Top | Side::Bottom => {
            if d.x >= 0.0 { Side::Left } else { Side::Right }
        }
    }
}

struct RouteInput {
    p1: Pos2,
    s1: Side,
    p2: Pos2,
    s2: Side,
    r1: Rect,
    r2: Rect,
    fan_src: bool,
    fan_dst: bool,
    bend: f32,
}

fn tp(p: Pos2) -> Pos2 {
    pos2(p.y, p.x)
}

fn ts(s: Side) -> Side {
    match s {
        Side::Top => Side::Left,
        Side::Left => Side::Top,
        Side::Bottom => Side::Right,
        Side::Right => Side::Bottom,
    }
}

fn tr(r: Rect) -> Rect {
    Rect::from_min_max(tp(r.min), tp(r.max))
}

fn route(i: &RouteInput) -> (Vec<Pos2>, Option<(Pos2, Axis)>) {
    if i.s1.is_horizontal() {
        return route_h(i);
    }
    // Transpose so the source side is horizontal, route, then transpose back.
    let t = RouteInput {
        p1: tp(i.p1),
        s1: ts(i.s1),
        p2: tp(i.p2),
        s2: ts(i.s2),
        r1: tr(i.r1),
        r2: tr(i.r2),
        fan_src: i.fan_src,
        fan_dst: i.fan_dst,
        bend: i.bend,
    };
    let (pts, handle) = route_h(&t);
    let handle = handle.map(|(p, axis)| (tp(p), if axis == Axis::X { Axis::Y } else { Axis::X }));
    (pts.into_iter().map(tp).collect(), handle)
}

/// Routing when the source side is Left or Right.
fn route_h(i: &RouteInput) -> (Vec<Pos2>, Option<(Pos2, Axis)>) {
    let (p1, p2) = (i.p1, i.p2);
    let d1 = i.s1.dir();
    let d2 = i.s2.dir();
    let a = p1 + d1 * STUB;
    let b = p2 + d2 * STUB;

    if i.s2.is_horizontal() {
        let ahead = (p2.x - p1.x) * d1.x;
        let opposite = d1.x != d2.x;
        if opposite && ahead > STUB * 0.5 {
            let gap = (p2.x - p1.x).abs();
            let base = if i.fan_src && !i.fan_dst {
                p1.x + d1.x * FAN_TRUNK.min(gap * 0.5)
            } else if i.fan_dst && !i.fan_src {
                p2.x + d2.x * FAN_TRUNK.min(gap * 0.5)
            } else {
                (p1.x + p2.x) * 0.5
            };
            let x = base + i.bend;
            let pts = vec![p1, pos2(x, p1.y), pos2(x, p2.y), p2];
            let handle = ((p1.y - p2.y).abs() > 1.0 || i.bend != 0.0).then(|| (pos2(x, (p1.y + p2.y) * 0.5), Axis::X));
            return (pts, handle);
        }
        if !opposite {
            // Both ports face the same way: wrap around the outside.
            let base = if d1.x > 0.0 { i.r1.max.x.max(i.r2.max.x) + STUB } else { i.r1.min.x.min(i.r2.min.x) - STUB };
            let x = base + i.bend;
            let pts = vec![p1, pos2(x, p1.y), pos2(x, p2.y), p2];
            return (pts, Some((pos2(x, (p1.y + p2.y) * 0.5), Axis::X)));
        }
        // Target is behind the source: loop around via a horizontal channel.
        let base = if i.r2.min.y > i.r1.max.y {
            (i.r1.max.y + i.r2.min.y) * 0.5
        } else if i.r1.min.y > i.r2.max.y {
            (i.r2.max.y + i.r1.min.y) * 0.5
        } else {
            i.r1.max.y.max(i.r2.max.y) + STUB
        };
        let y = base + i.bend;
        let pts = vec![p1, a, pos2(a.x, y), pos2(b.x, y), b, p2];
        return (pts, Some((pos2((a.x + b.x) * 0.5, y), Axis::Y)));
    }

    // Source horizontal, target vertical.
    let corner = pos2(p2.x, p1.y);
    let ok1 = (corner.x - p1.x) * d1.x >= STUB * 0.5;
    let ok2 = (p2.y - corner.y) * (-d2.y) >= STUB * 0.5;
    if ok1 && ok2 && i.bend == 0.0 {
        return (vec![p1, corner, p2], None);
    }
    let base = if ok1 { (p1.x + p2.x) * 0.5 } else { a.x };
    let x = base + i.bend;
    let pts = vec![p1, pos2(x, p1.y), pos2(x, b.y), b, p2];
    (pts, Some((pos2(x, (p1.y + b.y) * 0.5), Axis::X)))
}

/// Removes duplicate and collinear points.
pub fn simplify(points: Vec<Pos2>) -> Vec<Pos2> {
    let mut out: Vec<Pos2> = Vec::with_capacity(points.len());
    for p in points {
        if let Some(last) = out.last()
            && (*last - p).length_sq() < 0.01
        {
            continue;
        }
        while out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let collinear = ((a.x - b.x).abs() < 0.01 && (b.x - p.x).abs() < 0.01)
                || ((a.y - b.y).abs() < 0.01 && (b.y - p.y).abs() < 0.01);
            if collinear {
                out.pop();
            } else {
                break;
            }
        }
        out.push(p);
    }
    out
}

pub fn distance_to_polyline(points: &[Pos2], p: Pos2) -> f32 {
    points
        .windows(2)
        .map(|w| distance_to_segment(w[0], w[1], p))
        .fold(f32::INFINITY, f32::min)
}

fn distance_to_segment(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// Where an edge label sits for a given placement.
pub fn label_anchor(points: &[Pos2], pos: LabelPos) -> Pos2 {
    match pos {
        LabelPos::Auto => auto_label_anchor(points),
        LabelPos::Along(t) => point_along(points, t),
        LabelPos::SegMid(i) => {
            if points.len() < 2 {
                return points.first().copied().unwrap_or_default();
            }
            let i = i.min(points.len() - 2);
            points[i] + (points[i + 1] - points[i]) * 0.5
        }
    }
}

/// Default anchor: the segment entering the target if it is long enough
/// (it is unique per edge), otherwise the longest segment.
fn auto_label_anchor(points: &[Pos2]) -> Pos2 {
    if points.len() < 2 {
        return points.first().copied().unwrap_or_default();
    }
    let n = points.len();
    let last = (points[n - 2], points[n - 1]);
    if (last.1 - last.0).length() >= 40.0 || n == 2 {
        return last.0 + (last.1 - last.0) * 0.5;
    }
    let (a, b) = points
        .windows(2)
        .map(|w| (w[0], w[1]))
        .max_by(|x, y| (x.1 - x.0).length_sq().total_cmp(&(y.1 - y.0).length_sq()))
        .unwrap_or(last);
    a + (b - a) * 0.5
}

fn polyline_length(points: &[Pos2]) -> f32 {
    points.windows(2).map(|w| (w[1] - w[0]).length()).sum()
}

/// Point at fraction `t` (0..1) of the polyline's length.
pub fn point_along(points: &[Pos2], t: f32) -> Pos2 {
    let total = polyline_length(points);
    let Some(&first) = points.first() else { return Pos2::ZERO };
    if total <= 0.0 {
        return first;
    }
    let mut remaining = t.clamp(0.0, 1.0) * total;
    for w in points.windows(2) {
        let len = (w[1] - w[0]).length();
        if remaining <= len && len > 0.0 {
            return w[0] + (w[1] - w[0]) * (remaining / len);
        }
        remaining -= len;
    }
    *points.last().unwrap_or(&first)
}

/// Closest point on the polyline to `p`, as (fraction of total length, point).
pub fn project_along(points: &[Pos2], p: Pos2) -> (f32, Pos2) {
    let total = polyline_length(points);
    let Some(&first) = points.first() else { return (0.0, p) };
    let mut best = (f32::INFINITY, 0.0, first);
    let mut walked = 0.0;
    for w in points.windows(2) {
        let ab = w[1] - w[0];
        let len = ab.length();
        let t = if len > 0.0 { ((p - w[0]).dot(ab) / (len * len)).clamp(0.0, 1.0) } else { 0.0 };
        let q = w[0] + ab * t;
        let d = (p - q).length_sq();
        if d < best.0 {
            best = (d, walked + t * len, q);
        }
        walked += len;
    }
    let frac = if total > 0.0 { best.1 / total } else { 0.0 };
    (frac, best.2)
}

/// Midpoints of every segment, in order from the source.
pub fn segment_midpoints(points: &[Pos2]) -> Vec<Pos2> {
    points.windows(2).map(|w| w[0] + (w[1] - w[0]) * 0.5).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NodeShape, NodeStyle};
    use egui::vec2;

    fn diagram(transpose: bool) -> (Doc, u64, u64) {
        let mut doc = Doc::default();
        let mut add = |center| {
            let size = vec2(80.0, 40.0);
            doc.add_node(
                if transpose { tp(center) } else { center },
                if transpose { vec2(size.y, size.x) } else { size },
                NodeStyle::default(),
                "",
            )
        };
        let a = add(pos2(0.0, 0.0));
        let b = add(pos2(300.0, 100.0));
        let c = add(pos2(71.0, 300.0));
        let d = add(pos2(371.0, 400.0));
        let (from, to) = if transpose { (Side::Bottom, Side::Top) } else { (Side::Right, Side::Left) };
        let edge = doc.add_edge(a, b, Some(from), Some(to)).unwrap();
        let other = doc.add_edge(c, d, Some(from), Some(to)).unwrap();
        (doc, edge, other)
    }

    #[test]
    fn bends_align_with_parallel_segments_on_both_axes() {
        for transpose in [false, true] {
            let (mut doc, edge, other) = diagram(transpose);
            doc.edge_mut(edge).unwrap().bend = 19.0;
            let routes = compute_routes(&doc);
            let (bend, (a, b)) = snap_bend(&doc, &routes, edge, 68.0, 8.0).unwrap();
            assert_eq!(bend, 71.0);
            let (target, axis) = routes[&other].handle.unwrap();
            assert_eq!(axis.coordinate(a), axis.coordinate(target));
            assert_eq!(axis.coordinate(b), axis.coordinate(target));
            doc.edge_mut(edge).unwrap().bend = bend;
            let updated = compute_routes(&doc);
            assert_eq!(axis.coordinate(updated[&edge].handle.unwrap().0), axis.coordinate(target));
        }
    }

    #[test]
    fn bends_align_with_midpoints_of_every_node_shape() {
        for transpose in [false, true] {
            for shape in NodeShape::ALL {
                let (mut doc, edge, _) = diagram(transpose);
                let center = if transpose { pos2(250.0, 213.0) } else { pos2(213.0, 250.0) };
                doc.add_node(center, vec2(60.0, 40.0), NodeStyle { shape, ..NodeStyle::default() }, "");
                let routes = compute_routes(&doc);
                let (bend, _) = snap_bend(&doc, &routes, edge, 60.0, 8.0).unwrap();
                assert_eq!(bend, 63.0);
                doc.edge_mut(edge).unwrap().bend = bend;
                let updated = compute_routes(&doc);
                let (handle, axis) = updated[&edge].handle.unwrap();
                assert_eq!(axis.coordinate(handle), axis.coordinate(center));
            }
        }
    }

    #[test]
    fn closest_alignment_wins_over_a_connector() {
        let (mut doc, edge, _) = diagram(false);
        doc.add_node(pos2(216.0, 250.0), vec2(60.0, 40.0), NodeStyle::default(), "");
        let routes = compute_routes(&doc);
        assert_eq!(snap_bend(&doc, &routes, edge, 68.0, 8.0).unwrap().0, 66.0);
    }

    #[test]
    fn snap_threshold_is_constant_in_screen_space() {
        let (doc, edge, _) = diagram(false);
        let routes = compute_routes(&doc);
        for zoom in [0.5, 1.0, 2.0] {
            let tolerance = 8.0 / zoom;
            assert!(snap_bend(&doc, &routes, edge, 71.0 + tolerance, tolerance).is_some());
            assert!(snap_bend(&doc, &routes, edge, 71.0 + tolerance + 0.1, tolerance).is_none());
        }
    }

    #[test]
    fn dragged_route_does_not_snap_to_itself_or_perpendicular_segments() {
        let (doc, edge, _) = diagram(false);
        let routes = compute_routes(&doc);
        assert!(snap_bend(&doc, &routes, edge, 0.0, 8.0).is_none());
        assert!(snap_bend(&doc, &routes, edge, -35.0, 8.0).is_none());
    }
}