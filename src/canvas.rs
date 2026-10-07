use crate::app::{DecisiveApp, Drag, EditTarget, End, GRID, Handle};
use crate::model::{Node, Side};
use crate::presets::PRESETS;
use crate::render::{self, SELECT_COLOR, View, layout_text, text_wrap_width};
use crate::routing::{self, Axis, Route};
use egui::{Color32, CursorIcon, Event, FontId, Id, Key, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, Vec2, pos2, vec2};
use std::collections::{HashMap, HashSet};

const PORT_R: f32 = 4.5;
const PORT_HIT: f32 = 9.0;
/// Ports sit just outside the node so they never overlap the resize handles.
const PORT_OFF: f32 = 13.0;
const HANDLE_HIT: f32 = 7.0;
const EDGE_HIT: f32 = 6.0;
const GUIDE_COLOR: Color32 = Color32::from_rgb(0xE0, 0x3E, 0x9A);

#[derive(Clone, Copy, Debug)]
enum Hit {
    Resize(u64, Handle),
    EdgeEnd(u64, End),
    Bend(u64, Axis),
    Port(u64, Side),
    Node(u64),
    Edge(u64),
    None,
}

fn snap_pos(p: Pos2) -> Pos2 {
    pos2((p.x / GRID).round() * GRID, (p.y / GRID).round() * GRID)
}

fn consume_plain_enter(ctx: &egui::Context) -> bool {
    ctx.input_mut(|i| {
        let before = i.events.len();
        i.events
            .retain(|e| !matches!(e, Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if !modifiers.shift));
        before != i.events.len()
    })
}

impl DecisiveApp {
    fn hit(&self, routes: &HashMap<u64, Route>, screen: Pos2) -> Hit {
        let view = self.view();
        let world = view.to_world(screen);

        if self.sel_nodes.len() == 1
            && self.sel_edges.is_empty()
            && let Some(n) = self.sel_nodes.iter().next().and_then(|id| self.doc.node(*id))
        {
            let sr = view.rect_to_screen(n.rect()).expand(3.0);
            for h in Handle::ALL {
                if (h.pos(sr) - screen).length() <= HANDLE_HIT {
                    return Hit::Resize(n.id, h);
                }
            }
        }
        if self.sel_edges.len() == 1
            && let Some(id) = self.sel_edges.iter().next()
            && let Some(r) = routes.get(id)
        {
            if let (Some(a), Some(b)) = (r.points.first(), r.points.last()) {
                if (view.to_screen(*b) - screen).length() <= HANDLE_HIT {
                    return Hit::EdgeEnd(*id, End::To);
                }
                if (view.to_screen(*a) - screen).length() <= HANDLE_HIT {
                    return Hit::EdgeEnd(*id, End::From);
                }
            }
            if let Some((p, axis)) = r.handle
                && (view.to_screen(p) - screen).length() <= HANDLE_HIT
            {
                return Hit::Bend(*id, axis);
            }
        }
        if let Some((id, Some(side))) = self.port_under(screen, None) {
            return Hit::Port(id, side);
        }
        if let Some(n) = self.doc.nodes.iter().rev().find(|n| n.contains(world)) {
            return Hit::Node(n.id);
        }
        let tol = EDGE_HIT / self.zoom;
        for e in self.doc.edges.iter().rev() {
            if let Some(r) = routes.get(&e.id)
                && routing::distance_to_polyline(&r.points, world) <= tol
            {
                return Hit::Edge(e.id);
            }
        }
        Hit::None
    }

    /// Node (and port, if the pointer is on one) under a screen position.
    fn port_under(&self, screen: Pos2, exclude: Option<u64>) -> Option<(u64, Option<Side>)> {
        let view = self.view();
        let world = view.to_world(screen);
        for n in self.doc.nodes.iter().rev() {
            if Some(n.id) == exclude {
                continue;
            }
            let sr = view.rect_to_screen(n.rect()).expand(PORT_HIT + PORT_OFF);
            if !sr.contains(screen) {
                continue;
            }
            for s in Side::ALL {
                if (port_screen(&view, n, s) - screen).length() <= PORT_HIT {
                    return Some((n.id, Some(s)));
                }
            }
            if n.contains(world) {
                return Some((n.id, None));
            }
        }
        None
    }

    /// Node whose ports should be shown for a hover position.
    fn port_node_for_hover(&self, screen: Pos2) -> Option<u64> {
        let view = self.view();
        self.doc
            .nodes
            .iter()
            .rev()
            .find(|n| view.rect_to_screen(n.rect()).expand(PORT_HIT + PORT_OFF).contains(screen))
            .map(|n| n.id)
    }

    fn begin_drag(&mut self, ctx: &egui::Context, routes: &HashMap<u64, Route>, origin: Pos2) {
        let (space, shift, alt, cmd) = ctx.input(|i| (i.key_down(Key::Space), i.modifiers.shift, i.modifiers.alt, i.modifiers.command));
        if self.editing.is_some() {
            self.finish_edit();
        }
        let world = self.view().to_world(origin);
        if space {
            self.drag = Drag::Pan;
            return;
        }
        self.drag = match self.hit(routes, origin) {
            Hit::Resize(id, handle) => match self.doc.node(id) {
                Some(n) => Drag::Resize { id, handle, orig: n.rect() },
                None => Drag::None,
            },
            Hit::EdgeEnd(edge, end) => Drag::Reconnect { edge, end },
            Hit::Bend(edge, axis) => {
                let orig = self.doc.edge(edge).map(|e| e.bend).unwrap_or(0.0);
                Drag::Bend { edge, axis, start: world, orig }
            }
            Hit::Port(id, side) => Drag::Connect { from: id, side: Some(side) },
            Hit::Node(id) if alt => Drag::Connect { from: id, side: None },
            Hit::Node(id) => {
                if !self.sel_nodes.contains(&id) {
                    if shift || cmd {
                        self.sel_nodes.insert(id);
                    } else {
                        self.select_only_node(id);
                    }
                }
                let orig = self.doc.nodes.iter().filter(|n| self.sel_nodes.contains(&n.id)).map(|n| (n.id, n.pos)).collect();
                Drag::Move { anchor: id, start: world, orig }
            }
            Hit::Edge(id) => {
                self.select_only_edge(id);
                match routes.get(&id).and_then(|r| r.handle) {
                    Some((_, axis)) => {
                        let orig = self.doc.edge(id).map(|e| e.bend).unwrap_or(0.0);
                        Drag::Bend { edge: id, axis, start: world, orig }
                    }
                    None => Drag::None,
                }
            }
            Hit::None => {
                let base = if shift || cmd {
                    self.sel_nodes.clone()
                } else {
                    self.sel_edges.clear();
                    HashSet::new()
                };
                Drag::BoxSelect { start: world, base }
            }
        };
    }

    fn update_drag(&mut self, ctx: &egui::Context, pointer: Pos2, delta: Vec2) {
        let world = self.view().to_world(pointer);
        let alt = ctx.input(|i| i.modifiers.alt);
        match self.drag.clone() {
            Drag::Pan => self.pan += delta,
            Drag::Move { anchor, start, orig } => {
                let Some(&(_, apos)) = orig.iter().find(|(id, _)| *id == anchor) else { return };
                let Some(asize) = self.doc.node(anchor).map(|n| n.size) else { return };
                let orig_center = apos + asize * 0.5;
                let mut c = orig_center + (world - start);
                if self.snap && !alt {
                    c = snap_pos(c);
                }
                // Align with other nodes' centers so connectors come out straight.
                let moving: HashSet<u64> = orig.iter().map(|(id, _)| *id).collect();
                let thr = 6.0 / self.zoom;
                let mut best_x: Option<(f32, f32, Rect)> = None;
                let mut best_y: Option<(f32, f32, Rect)> = None;
                if !alt {
                    for n in self.doc.nodes.iter().filter(|n| !moving.contains(&n.id)) {
                        let nc = n.center();
                        let dx = (nc.x - c.x).abs();
                        if dx < thr && best_x.is_none_or(|b| dx < b.0) {
                            best_x = Some((dx, nc.x, n.rect()));
                        }
                        let dy = (nc.y - c.y).abs();
                        if dy < thr && best_y.is_none_or(|b| dy < b.0) {
                            best_y = Some((dy, nc.y, n.rect()));
                        }
                    }
                }
                self.guides.clear();
                if let Some((_, x, r)) = best_x {
                    c.x = x;
                    let me = Rect::from_center_size(c, asize);
                    self.guides.push((pos2(x, r.min.y.min(me.min.y)), pos2(x, r.max.y.max(me.max.y))));
                }
                if let Some((_, y, r)) = best_y {
                    c.y = y;
                    let me = Rect::from_center_size(c, asize);
                    self.guides.push((pos2(r.min.x.min(me.min.x), y), pos2(r.max.x.max(me.max.x), y)));
                }
                let d = c - orig_center;
                for (id, p) in orig {
                    if let Some(n) = self.doc.node_mut(id) {
                        n.pos = p + d;
                    }
                }
            }
            Drag::Resize { id, handle, orig } => {
                let p = if self.snap && !alt { snap_pos(world) } else { world };
                let (dx, dy) = handle.dir();
                let mut r = orig;
                if dx < 0 {
                    r.min.x = p.x.min(r.max.x - 20.0);
                }
                if dx > 0 {
                    r.max.x = p.x.max(r.min.x + 20.0);
                }
                if dy < 0 {
                    r.min.y = p.y.min(r.max.y - 14.0);
                }
                if dy > 0 {
                    r.max.y = p.y.max(r.min.y + 14.0);
                }
                if let Some(n) = self.doc.node_mut(id) {
                    n.pos = r.min;
                    n.size = r.size();
                }
            }
            Drag::BoxSelect { start, base } => {
                let r = Rect::from_two_pos(start, world);
                let mut sel = base;
                sel.extend(self.doc.nodes.iter().filter(|n| r.intersects(n.rect())).map(|n| n.id));
                self.sel_nodes = sel;
            }
            Drag::Bend { edge, axis, start, orig } => {
                let d = world - start;
                let mut bend = orig + if axis == Axis::X { d.x } else { d.y };
                if self.snap && !alt {
                    bend = (bend / (GRID * 0.5)).round() * GRID * 0.5;
                }
                if let Some(e) = self.doc.edge_mut(edge) {
                    e.bend = bend;
                }
            }
            Drag::Connect { .. } | Drag::Reconnect { .. } | Drag::None => {}
        }
    }

    fn end_drag(&mut self, pointer: Pos2) {
        let world = self.view().to_world(pointer);
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Connect { from, side } => match self.port_under(pointer, Some(from)) {
                Some((to, to_side)) => {
                    if let Some(id) = self.doc.add_edge(from, to, side, to_side) {
                        self.select_only_edge(id);
                    }
                }
                None => self.spawn_connected(from, side, world),
            },
            Drag::Reconnect { edge, end } => {
                if let Some((n, s)) = self.port_under(pointer, None)
                    && let Some(e) = self.doc.edge_mut(edge)
                {
                    match end {
                        End::To if n != e.from => {
                            e.to = n;
                            e.to_side = s;
                            e.bend = 0.0;
                        }
                        End::From if n != e.to => {
                            e.from = n;
                            e.from_side = s;
                            e.bend = 0.0;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        self.guides.clear();
    }

    /// Creates a new step where a connection was dropped on empty canvas.
    fn spawn_connected(&mut self, from: u64, side: Option<Side>, world: Pos2) {
        let Some(src) = self.doc.node(from).cloned() else { return };
        let (style, size) = self.child_style(&src);
        let dir_side = side.unwrap_or_else(|| {
            let d = world - src.center();
            if d.x.abs() >= d.y.abs() {
                if d.x >= 0.0 { Side::Right } else { Side::Left }
            } else if d.y >= 0.0 {
                Side::Bottom
            } else {
                Side::Top
            }
        });
        let mut center = world + dir_side.dir() * Vec2::new(size.x * 0.5, size.y * 0.5);
        if self.snap {
            center = snap_pos(center);
        }
        let sc = src.center();
        if dir_side.is_horizontal() && (center.y - sc.y).abs() < 15.0 {
            center.y = sc.y;
        }
        if !dir_side.is_horizontal() && (center.x - sc.x).abs() < 15.0 {
            center.x = sc.x;
        }
        let id = self.doc.add_node(center, size, style, "");
        self.doc.add_edge(from, id, side, None);
        self.select_only_node(id);
        self.start_edit(EditTarget::Node(id));
    }

    fn click(&mut self, ctx: &egui::Context, routes: &HashMap<u64, Route>, pointer: Pos2) {
        let toggle = ctx.input(|i| i.modifiers.shift || i.modifiers.command);
        if self.editing.is_some() {
            self.finish_edit();
        }
        match self.hit(routes, pointer) {
            Hit::Node(id) | Hit::Port(id, _) => {
                if toggle {
                    if !self.sel_nodes.remove(&id) {
                        self.sel_nodes.insert(id);
                    }
                } else {
                    self.select_only_node(id);
                }
            }
            Hit::Edge(id) | Hit::Bend(id, _) | Hit::EdgeEnd(id, _) => {
                if toggle {
                    if !self.sel_edges.remove(&id) {
                        self.sel_edges.insert(id);
                    }
                } else {
                    self.select_only_edge(id);
                }
            }
            Hit::Resize(..) => {}
            Hit::None => {
                if !toggle {
                    self.sel_nodes.clear();
                    self.sel_edges.clear();
                }
            }
        }
    }

    fn double_click(&mut self, routes: &HashMap<u64, Route>, pointer: Pos2) {
        match self.hit(routes, pointer) {
            Hit::Node(id) | Hit::Port(id, _) | Hit::Resize(id, _) => {
                self.select_only_node(id);
                self.start_edit(EditTarget::Node(id));
            }
            Hit::Edge(id) | Hit::Bend(id, _) | Hit::EdgeEnd(id, _) => {
                self.select_only_edge(id);
                self.start_edit(EditTarget::EdgeLabel(id));
            }
            Hit::None => {
                let mut c = self.view().to_world(pointer);
                if self.snap {
                    c = snap_pos(c);
                }
                let p = &PRESETS[self.active_preset];
                let id = self.doc.add_node(c, p.size(), p.style(), "");
                self.select_only_node(id);
                self.start_edit(EditTarget::Node(id));
            }
        }
    }

    fn context_menu(&mut self, ui: &mut egui::Ui) {
        use crate::app::{ChildDir, Cmd};
        let ctx = ui.ctx().clone();
        let mut cmd: Option<Cmd> = None;
        let mut item = |ui: &mut egui::Ui, label: &str, c: Cmd| {
            if ui.button(label).clicked() {
                cmd = Some(c);
                ui.close();
            }
        };
        if !self.sel_nodes.is_empty() {
            item(ui, "✏ Edit text", Cmd::EditText);
            item(ui, "→ Add next step (Tab)", Cmd::AddChild(ChildDir::Right));
            item(ui, "↓ Add branch below (Shift+Tab)", Cmd::AddChild(ChildDir::Down));
            ui.separator();
            item(ui, "Copy style", Cmd::CopyStyle);
            item(ui, "Paste style", Cmd::PasteStyle);
            item(ui, "Duplicate", Cmd::Duplicate);
            item(ui, "Bring to front", Cmd::BringToFront);
            item(ui, "Send to back", Cmd::SendToBack);
            ui.separator();
            item(ui, "🗑 Delete", Cmd::Delete);
        } else if !self.sel_edges.is_empty() {
            item(ui, "✏ Edit label", Cmd::EditText);
            ui.separator();
            item(ui, "🗑 Delete", Cmd::Delete);
        } else {
            let mut add: Option<usize> = None;
            ui.menu_button("Add shape here", |ui| {
                for (i, p) in PRESETS.iter().enumerate() {
                    if ui.button(p.name).clicked() {
                        add = Some(i);
                        ui.close();
                    }
                }
            });
            if let Some(i) = add {
                let p = &PRESETS[i];
                let mut c = self.context_pos;
                if self.snap {
                    c = snap_pos(c);
                }
                let id = self.doc.add_node(c, p.size(), p.style(), "");
                self.active_preset = i;
                self.select_only_node(id);
                self.start_edit(EditTarget::Node(id));
            }
            item(ui, "Select all", Cmd::SelectAll);
            item(ui, "Zoom to fit", Cmd::ZoomFit);
        }
        if let Some(c) = cmd {
            self.run(&ctx, c);
        }
    }

    pub fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.canvas_rect = rect;
        if self.fit_pending {
            self.fit_pending = false;
            self.zoom_fit();
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0, Color32::WHITE);

        // ----- Pan / zoom -----
        let hover = ctx.input(|i| i.pointer.hover_pos());
        if let Some(h) = hover
            && rect.contains(h)
            && ui.ctx().layer_id_at(h).is_none_or(|l| l == ui.layer_id())
        {
            let (scroll, zoom_delta) = ctx.input(|i| (i.smooth_scroll_delta(), i.zoom_delta()));
            if zoom_delta != 1.0 {
                self.zoom_about(h, self.zoom * zoom_delta);
            } else if scroll != Vec2::ZERO {
                self.pan += scroll;
            }
        }
        if resp.dragged_by(PointerButton::Middle) {
            self.pan += resp.drag_delta();
        }

        // ----- Interaction -----
        let routes = routing::compute_routes(&self.doc);
        let pointer = ctx.input(|i| i.pointer.interact_pos()).unwrap_or(rect.center());
        if resp.drag_started_by(PointerButton::Primary) {
            let origin = ctx.input(|i| i.pointer.press_origin()).unwrap_or(pointer);
            self.begin_drag(&ctx, &routes, origin);
        }
        if resp.dragged_by(PointerButton::Primary) {
            self.update_drag(&ctx, pointer, resp.drag_delta());
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            self.end_drag(pointer);
        }
        if resp.clicked() {
            self.click(&ctx, &routes, pointer);
        }
        if resp.double_clicked() {
            self.double_click(&routes, pointer);
        }
        if resp.secondary_clicked() {
            self.context_pos = self.view().to_world(pointer);
            if self.editing.is_some() {
                self.finish_edit();
            }
            match self.hit(&routes, pointer) {
                Hit::Node(id) | Hit::Port(id, _) => {
                    if !self.sel_nodes.contains(&id) {
                        self.select_only_node(id);
                    }
                }
                Hit::Edge(id) | Hit::Bend(id, _) | Hit::EdgeEnd(id, _) => {
                    if !self.sel_edges.contains(&id) {
                        self.select_only_edge(id);
                    }
                }
                _ => {
                    self.sel_nodes.clear();
                    self.sel_edges.clear();
                }
            }
        }
        resp.context_menu(|ui| self.context_menu(ui));

        // Hover cursor feedback.
        if matches!(self.drag, Drag::None)
            && let Some(h) = hover
            && rect.contains(h)
        {
            let icon = match self.hit(&routes, h) {
                Hit::Resize(_, handle) => handle.cursor(),
                Hit::Port(..) => CursorIcon::Crosshair,
                Hit::EdgeEnd(..) => CursorIcon::Grab,
                Hit::Bend(_, Axis::X) => CursorIcon::ResizeHorizontal,
                Hit::Bend(_, Axis::Y) => CursorIcon::ResizeVertical,
                Hit::Node(_) => CursorIcon::Move,
                Hit::Edge(_) => CursorIcon::PointingHand,
                Hit::None => CursorIcon::Default,
            };
            ctx.set_cursor_icon(icon);
        } else if matches!(self.drag, Drag::Pan) || resp.dragged_by(PointerButton::Middle) {
            ctx.set_cursor_icon(CursorIcon::Grabbing);
        }

        // ----- Drawing -----
        let routes = routing::compute_routes(&self.doc);
        let view = self.view();
        if self.show_grid {
            paint_grid(&painter, &view, rect);
        }

        let reconnecting = match self.drag {
            Drag::Reconnect { edge, .. } => Some(edge),
            _ => None,
        };
        for e in &self.doc.edges {
            if Some(e.id) == reconnecting {
                continue;
            }
            if let Some(r) = routes.get(&e.id) {
                let hl = self.sel_edges.contains(&e.id).then_some(SELECT_COLOR);
                render::paint_edge(&painter, &view, e, r, hl);
            }
        }
        let editing_label = match self.editing {
            Some(EditTarget::EdgeLabel(id)) => Some(id),
            _ => None,
        };
        for e in &self.doc.edges {
            if Some(e.id) == editing_label || Some(e.id) == reconnecting {
                continue;
            }
            if let Some(r) = routes.get(&e.id) {
                render::paint_edge_label(&painter, &view, e, routing::label_anchor(&r.points), &ctx, &self.fonts);
            }
        }

        let editing_node = match self.editing {
            Some(EditTarget::Node(id)) => Some(id),
            _ => None,
        };
        let visible = rect.expand(20.0);
        for n in &self.doc.nodes {
            let sr = view.rect_to_screen(n.rect());
            if !visible.intersects(sr) {
                continue;
            }
            let galley = (Some(n.id) != editing_node && !n.text.is_empty() && n.style.font_size * self.zoom >= 2.5)
                .then(|| layout_text(&ctx, &self.fonts, &n.text, &n.style, text_wrap_width(n), self.zoom));
            render::paint_node(&painter, &view, n, galley);
        }

        // Selection & hover decorations.
        for n in self.doc.nodes.iter().filter(|n| self.sel_nodes.contains(&n.id)) {
            render::paint_selection_outline(&painter, &view, n, SELECT_COLOR, 1.5);
        }
        if self.sel_nodes.len() == 1
            && self.sel_edges.is_empty()
            && let Some(n) = self.sel_nodes.iter().next().and_then(|id| self.doc.node(*id))
        {
            let sr = view.rect_to_screen(n.rect()).expand(3.0);
            for h in Handle::ALL {
                let r = Rect::from_center_size(h.pos(sr), Vec2::splat(7.0));
                painter.rect(r, 1, Color32::WHITE, Stroke::new(1.2, SELECT_COLOR), StrokeKind::Middle);
            }
        }

        // Ports: on the hovered node, or on the drop target while connecting.
        let (port_node, active_port, exclude) = match self.drag {
            Drag::Connect { from, .. } => {
                let t = self.port_under(pointer, Some(from));
                (t.map(|t| t.0), t.and_then(|t| t.1), Some(from))
            }
            Drag::Reconnect { .. } => {
                let t = self.port_under(pointer, None);
                (t.map(|t| t.0), t.and_then(|t| t.1), None)
            }
            Drag::None => {
                let n = hover.filter(|h| rect.contains(*h)).and_then(|h| self.port_node_for_hover(h));
                let p = hover.and_then(|h| self.port_under(h, None)).and_then(|t| t.1);
                (n, p, None)
            }
            _ => (None, None, None),
        };
        let _ = exclude;
        if let Some(n) = port_node.and_then(|id| self.doc.node(id)) {
            if !matches!(self.drag, Drag::None) {
                render::paint_selection_outline(&painter, &view, n, SELECT_COLOR.gamma_multiply(0.6), 2.0);
            }
            for s in Side::ALL {
                let p = port_screen(&view, n, s);
                painter.line_segment([view.to_screen(n.port(s)), p], Stroke::new(1.0, SELECT_COLOR.gamma_multiply(0.5)));
                let active = active_port == Some(s);
                let fill = if active { SELECT_COLOR } else { Color32::WHITE };
                painter.circle(p, if active { PORT_R + 1.5 } else { PORT_R }, fill, Stroke::new(1.5, SELECT_COLOR));
            }
        }

        // Selected connector handles.
        if self.sel_edges.len() == 1
            && reconnecting.is_none()
            && let Some(r) = self.sel_edges.iter().next().and_then(|id| routes.get(id))
        {
            if let (Some(a), Some(b)) = (r.points.first(), r.points.last()) {
                for p in [a, b] {
                    painter.circle(view.to_screen(*p), 5.0, SELECT_COLOR, Stroke::new(1.5, Color32::WHITE));
                }
            }
            if let Some((p, _)) = r.handle {
                let hr = Rect::from_center_size(view.to_screen(p), Vec2::splat(9.0));
                painter.rect(hr, 1, Color32::WHITE, Stroke::new(1.5, SELECT_COLOR), StrokeKind::Middle);
            }
        }

        // Live previews.
        let world = view.to_world(pointer);
        let preview = Stroke::new(1.5, SELECT_COLOR);
        match &self.drag {
            Drag::Connect { from, side } => {
                if let Some(src) = self.doc.node(*from) {
                    let target = self.port_under(pointer, Some(*from));
                    let tn: Option<(&Node, Option<Side>)> = target.and_then(|(id, s)| self.doc.node(id).map(|n| (n, s)));
                    let pts: Vec<Pos2> = routing::preview_route(src, *side, world, tn).into_iter().map(|p| view.to_screen(p)).collect();
                    render::paint_polyline(&painter, &pts, preview, true, true, self.zoom);
                }
            }
            Drag::Reconnect { edge, end } => {
                if let Some(e) = self.doc.edge(*edge) {
                    let target = self.port_under(pointer, None);
                    let tn: Option<(&Node, Option<Side>)> = target.and_then(|(id, s)| self.doc.node(id).map(|n| (n, s)));
                    let (fixed, fixed_side) = match end {
                        End::To => (e.from, e.from_side),
                        End::From => (e.to, e.to_side),
                    };
                    if let Some(src) = self.doc.node(fixed) {
                        let mut pts: Vec<Pos2> = routing::preview_route(src, fixed_side, world, tn).into_iter().map(|p| view.to_screen(p)).collect();
                        let arrow_at_pointer = *end == End::To;
                        if !arrow_at_pointer {
                            pts.reverse();
                        }
                        render::paint_polyline(&painter, &pts, preview, true, true, self.zoom);
                    }
                }
            }
            Drag::BoxSelect { start, .. } => {
                let r = view.rect_to_screen(Rect::from_two_pos(*start, world));
                painter.rect(r, 0, SELECT_COLOR.gamma_multiply(0.08), Stroke::new(1.0, SELECT_COLOR), StrokeKind::Middle);
            }
            _ => {}
        }
        for (a, b) in &self.guides {
            painter.line_segment([view.to_screen(*a), view.to_screen(*b)], Stroke::new(1.0, GUIDE_COLOR));
        }

        self.text_overlay(ui, &routes);
    }

    fn text_overlay(&mut self, ui: &mut egui::Ui, routes: &HashMap<u64, Route>) {
        let ctx = ui.ctx().clone();
        let view = self.view();
        match self.editing {
            Some(EditTarget::Node(id)) => {
                let Some(idx) = self.doc.nodes.iter().position(|n| n.id == id) else { return };
                let node = &self.doc.nodes[idx];
                let sr = view.rect_to_screen(node.rect());
                let wrap = (text_wrap_width(node) * self.zoom).max(20.0);
                let font = FontId::new(node.style.font_size * self.zoom, self.fonts.family(&node.style.font, node.style.bold));
                let color = node.style.text_color;
                let sample = if node.text.is_empty() { "M" } else { node.text.as_str() };
                let g = layout_text(&ctx, &self.fonts, sample, &node.style, text_wrap_width(node), self.zoom);
                let erect = Rect::from_center_size(sr.center(), vec2(wrap, g.rect.height() + 2.0));
                let edit_id = Id::new("dt_node_edit");
                let done = ctx.memory(|m| m.has_focus(edit_id)) && consume_plain_enter(&ctx);
                let text = &mut self.doc.nodes[idx].text;
                let r = ui.put(
                    erect,
                    TextEdit::multiline(text)
                        .id(edit_id)
                        .font(font)
                        .text_color(color)
                        .horizontal_align(egui::Align::Center)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .desired_width(wrap)
                        .desired_rows(1),
                );
                if self.edit_focus_pending {
                    r.request_focus();
                    self.edit_focus_pending = false;
                } else if done || r.lost_focus() {
                    self.finish_edit();
                }
            }
            Some(EditTarget::EdgeLabel(id)) => {
                let Some(anchor) = routes.get(&id).map(|r| routing::label_anchor(&r.points)) else { return };
                let Some(e) = self.doc.edge_mut(id) else { return };
                let font = FontId::new(e.style.font_size * self.zoom, self.fonts.family(crate::presets::DEFAULT_FONT, false));
                let c = view.to_screen(anchor);
                let erect = Rect::from_center_size(c, vec2(120.0, e.style.font_size * self.zoom * 1.6 + 4.0));
                ui.painter().rect(erect, 2, Color32::WHITE, Stroke::new(1.0, SELECT_COLOR), StrokeKind::Outside);
                let r = ui.put(
                    erect,
                    TextEdit::singleline(&mut e.label)
                        .id(Id::new("dt_label_edit"))
                        .font(font)
                        .horizontal_align(egui::Align::Center)
                        .frame(egui::Frame::NONE)
                        .hint_text("label"),
                );
                if self.edit_focus_pending {
                    r.request_focus();
                    self.edit_focus_pending = false;
                } else if r.lost_focus() {
                    self.finish_edit();
                }
            }
            None => {}
        }
    }
}

fn port_screen(view: &View, n: &Node, s: Side) -> Pos2 {
    view.to_screen(n.port(s)) + s.dir() * PORT_OFF
}

fn paint_grid(painter: &egui::Painter, view: &View, rect: Rect) {
    let mut step = GRID;
    while step * view.zoom < 9.0 {
        step *= 5.0;
    }
    let major = step * 5.0;
    let tl = view.to_world(rect.min);
    let br = view.to_world(rect.max);
    let minor_c = Color32::from_gray(244);
    let major_c = Color32::from_gray(228);
    let mut x = (tl.x / step).floor() * step;
    while x <= br.x {
        let sx = view.to_screen(pos2(x, 0.0)).x;
        let is_major = (x / major).round() * major == x;
        painter.line_segment([pos2(sx, rect.min.y), pos2(sx, rect.max.y)], Stroke::new(1.0, if is_major { major_c } else { minor_c }));
        x += step;
    }
    let mut y = (tl.y / step).floor() * step;
    while y <= br.y {
        let sy = view.to_screen(pos2(0.0, y)).y;
        let is_major = (y / major).round() * major == y;
        painter.line_segment([pos2(rect.min.x, sy), pos2(rect.max.x, sy)], Stroke::new(1.0, if is_major { major_c } else { minor_c }));
        y += step;
    }
}
