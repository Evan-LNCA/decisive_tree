use crate::export::{self, ExportFormat};
use crate::fonts::{self, FontRegistry};
use crate::model::{Doc, Edge, FILE_EXT, LabelPos, Node, NodeShape, NodeStyle, Side};
use crate::presets::PRESETS;
use crate::render::View;
use crate::routing::{self, Axis};
use egui::{Color32, Event, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Vec2, ViewportCommand, vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const GRID: f32 = 10.0;
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 5.0;
const UNDO_LIMIT: usize = 300;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Handle {
    pub const ALL: [Handle; 8] = [Handle::N, Handle::S, Handle::E, Handle::W, Handle::NE, Handle::NW, Handle::SE, Handle::SW];

    /// (-1|0|1, -1|0|1) direction of the handle relative to the rect center.
    pub fn dir(self) -> (i8, i8) {
        match self {
            Handle::N => (0, -1),
            Handle::S => (0, 1),
            Handle::E => (1, 0),
            Handle::W => (-1, 0),
            Handle::NE => (1, -1),
            Handle::NW => (-1, -1),
            Handle::SE => (1, 1),
            Handle::SW => (-1, 1),
        }
    }

    pub fn pos(self, r: Rect) -> Pos2 {
        let (dx, dy) = self.dir();
        let x = match dx {
            -1 => r.min.x,
            0 => r.center().x,
            _ => r.max.x,
        };
        let y = match dy {
            -1 => r.min.y,
            0 => r.center().y,
            _ => r.max.y,
        };
        Pos2::new(x, y)
    }

    pub fn cursor(self) -> egui::CursorIcon {
        match self {
            Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
            Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
            Handle::NE | Handle::SW => egui::CursorIcon::ResizeNeSw,
            Handle::NW | Handle::SE => egui::CursorIcon::ResizeNwSe,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum End {
    From,
    To,
}

#[derive(Clone, Debug)]
pub enum Drag {
    None,
    Pan,
    Move { anchor: u64, start: Pos2, orig: Vec<(u64, Pos2)> },
    Resize { id: u64, handle: Handle, orig: Rect },
    BoxSelect { start: Pos2, base: HashSet<u64> },
    Connect { from: u64, side: Option<Side> },
    Reconnect { edge: u64, end: End },
    Bend { edge: u64, axis: Axis, start: Pos2, orig: f32 },
    Label { edge: u64 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditTarget {
    Node(u64),
    EdgeLabel(u64),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pending {
    New,
    Open,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChildDir {
    Right,
    Down,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    New,
    Open,
    Save,
    SaveAs,
    Export(ExportFormat),
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    ZoomFit,
    Zoom100,
    ZoomIn,
    ZoomOut,
    AddChild(ChildDir),
    EditText,
    CopyStyle,
    PasteStyle,
    BringToFront,
    SendToBack,
    Nudge(Vec2),
    Quit,
}

#[derive(Serialize, Deserialize)]
struct ClipData {
    decisive_tree_clip: u32,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

pub struct DecisiveApp {
    pub doc: Doc,
    committed: Doc,
    saved: Doc,
    undo: Vec<Doc>,
    redo: Vec<Doc>,
    pub path: Option<PathBuf>,
    pub fonts: FontRegistry,
    pub pan: Vec2,
    pub zoom: f32,
    pub sel_nodes: HashSet<u64>,
    pub sel_edges: HashSet<u64>,
    pub drag: Drag,
    pub editing: Option<EditTarget>,
    pub edit_focus_pending: bool,
    pub active_preset: usize,
    pub paste_count: u32,
    pub snap: bool,
    pub show_grid: bool,
    pub guides: Vec<(Pos2, Pos2)>,
    pub canvas_rect: Rect,
    pub context_pos: Pos2,
    pub fit_pending: bool,
    style_clipboard: Option<NodeStyle>,
    status: Option<(String, f64)>,
    show_help: bool,
    confirm: Option<Pending>,
    allow_close: bool,
    title: String,
    /// Tab press captured before egui (Some(shift)), so egui never uses Tab for focus navigation.
    tab_press: Option<bool>,
}

impl DecisiveApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let fonts = fonts::install(&cc.egui_ctx);
        let mut app = Self {
            doc: Doc::default(),
            committed: Doc::default(),
            saved: Doc::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            path: None,
            fonts,
            pan: vec2(40.0, 40.0),
            zoom: 1.0,
            sel_nodes: HashSet::new(),
            sel_edges: HashSet::new(),
            drag: Drag::None,
            editing: None,
            edit_focus_pending: false,
            active_preset: 0,
            paste_count: 0,
            snap: true,
            show_grid: true,
            guides: Vec::new(),
            canvas_rect: Rect::NOTHING,
            context_pos: Pos2::ZERO,
            fit_pending: false,
            style_clipboard: None,
            status: None,
            show_help: false,
            confirm: None,
            allow_close: false,
            title: String::new(),
            tab_press: None,
        };
        match open {
            Some(p) => app.load(&p),
            None => app.load_sample(),
        }
        app
    }

    fn load_sample(&mut self) {
        let mut d = Doc::default();
        let p = |i: usize| PRESETS[i].style();
        let s = |i: usize| PRESETS[i].size();
        let h = d.add_node(Pos2::new(60.0, 0.0), s(10), p(10), "Example");
        let a = d.add_node(Pos2::new(50.0, 80.0), s(0), p(0), "Start: Unit fails test");
        let q = d.add_node(Pos2::new(200.0, 80.0), s(4), p(4), "Pass?");
        let y = d.add_node(Pos2::new(340.0, 30.0), s(6), p(6), "Deliver to Production");
        let n = d.add_node(Pos2::new(340.0, 130.0), s(5), p(5), "Red Tag for Tier 2");
        let _ = h;
        d.add_edge(a, q, None, None);
        if let Some(e) = d.add_edge(q, y, Some(Side::Right), None) {
            d.edge_mut(e).unwrap().label = "Yes".into();
        }
        if let Some(e) = d.add_edge(q, n, Some(Side::Right), None) {
            d.edge_mut(e).unwrap().label = "No".into();
        }
        self.doc = d;
        self.committed = self.doc.clone();
        self.saved = self.doc.clone();
        self.fit_pending = true;
    }

    pub fn set_status(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.status = Some((msg.into(), ctx.input(|i| i.time)));
    }

    pub fn dirty(&self) -> bool {
        self.doc != self.saved
    }

    pub fn view(&self) -> View {
        View { origin: self.canvas_rect.min, pan: self.pan, zoom: self.zoom }
    }

    // ---------- Undo ----------

    /// Records a snapshot whenever the document settled into a new state.
    fn commit_if_changed(&mut self, ctx: &egui::Context) {
        let busy = ctx.input(|i| i.pointer.any_down()) || self.editing.is_some() || ctx.memory(|m| m.focused().is_some());
        if busy || self.doc == self.committed {
            return;
        }
        let prev = std::mem::replace(&mut self.committed, self.doc.clone());
        self.undo.push(prev);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) {
        self.finish_edit();
        if self.doc != self.committed {
            // Uncommitted change in flight: revert it first.
            self.redo.push(std::mem::replace(&mut self.doc, self.committed.clone()));
        } else if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, prev));
            self.committed = self.doc.clone();
        }
        self.prune_selection();
    }

    fn redo(&mut self) {
        self.finish_edit();
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, next));
            self.committed = self.doc.clone();
        }
        self.prune_selection();
    }

    fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.committed = self.doc.clone();
        self.saved = self.doc.clone();
        self.sel_nodes.clear();
        self.sel_edges.clear();
        self.editing = None;
        self.drag = Drag::None;
    }

    pub fn prune_selection(&mut self) {
        let nodes: HashSet<u64> = self.doc.nodes.iter().map(|n| n.id).collect();
        let edges: HashSet<u64> = self.doc.edges.iter().map(|e| e.id).collect();
        self.sel_nodes.retain(|id| nodes.contains(id));
        self.sel_edges.retain(|id| edges.contains(id));
        if let Some(t) = self.editing {
            let alive = match t {
                EditTarget::Node(id) => nodes.contains(&id),
                EditTarget::EdgeLabel(id) => edges.contains(&id),
            };
            if !alive {
                self.editing = None;
            }
        }
    }

    // ---------- Editing helpers ----------

    pub fn start_edit(&mut self, target: EditTarget) {
        self.editing = Some(target);
        self.edit_focus_pending = true;
    }

    pub fn finish_edit(&mut self) {
        self.editing = None;
        self.edit_focus_pending = false;
    }

    pub fn select_only_node(&mut self, id: u64) {
        self.sel_nodes.clear();
        self.sel_edges.clear();
        self.sel_nodes.insert(id);
    }

    pub fn select_only_edge(&mut self, id: u64) {
        self.sel_nodes.clear();
        self.sel_edges.clear();
        self.sel_edges.insert(id);
    }

    fn single_selected_node(&self) -> Option<u64> {
        if self.sel_nodes.len() == 1 { self.sel_nodes.iter().next().copied() } else { None }
    }

    /// Style for a node spawned from `src`: continue the same kind of step,
    /// except decisions/headers which spawn the active preset instead.
    pub(crate) fn child_style(&self, src: &Node) -> (NodeStyle, Vec2) {
        match src.style.shape {
            NodeShape::Diamond | NodeShape::Text => {
                let p = &PRESETS[self.active_preset];
                let mut st = p.style();
                if p.shape == NodeShape::Diamond || p.shape == NodeShape::Text {
                    st = PRESETS[0].style();
                    return (st, PRESETS[0].size());
                }
                st.font = src.style.font.clone();
                (st, p.size())
            }
            _ => (src.style.clone(), src.size),
        }
    }

    fn overlaps_any(&self, r: Rect) -> bool {
        self.doc.nodes.iter().any(|n| n.rect().expand(8.0).intersects(r))
    }

    pub fn add_child(&mut self, dir: ChildDir) {
        let Some(src_id) = self.single_selected_node() else { return };
        let Some(src) = self.doc.node(src_id).cloned() else { return };
        let (style, size) = self.child_style(&src);
        let r = src.rect();
        let mut center = match dir {
            ChildDir::Right => Pos2::new(r.max.x + 60.0 + size.x * 0.5, r.center().y),
            ChildDir::Down => Pos2::new(r.center().x, r.max.y + 50.0 + size.y * 0.5),
        };
        for _ in 0..50 {
            if !self.overlaps_any(Rect::from_center_size(center, size)) {
                break;
            }
            match dir {
                ChildDir::Right => center.y += size.y + 30.0,
                ChildDir::Down => center.x += size.x + 30.0,
            }
        }
        let id = self.doc.add_node(center, size, style, "");
        let side = match dir {
            ChildDir::Right => Side::Right,
            ChildDir::Down => Side::Bottom,
        };
        self.doc.add_edge(src_id, id, Some(side), None);
        self.select_only_node(id);
        self.start_edit(EditTarget::Node(id));
        self.ensure_visible(Rect::from_center_size(center, size));
    }

    pub fn ensure_visible(&mut self, world: Rect) {
        let view = self.view();
        let s = view.rect_to_screen(world);
        let c = self.canvas_rect.shrink(30.0);
        if c.width() <= 0.0 || c.contains_rect(s) {
            return;
        }
        let mut d = Vec2::ZERO;
        if s.max.x > c.max.x {
            d.x = c.max.x - s.max.x;
        }
        if s.min.x < c.min.x {
            d.x = c.min.x - s.min.x;
        }
        if s.max.y > c.max.y {
            d.y = c.max.y - s.max.y;
        }
        if s.min.y < c.min.y {
            d.y = c.min.y - s.min.y;
        }
        self.pan += d;
    }

    pub fn delete_selection(&mut self) {
        let nodes = std::mem::take(&mut self.sel_nodes);
        let edges = std::mem::take(&mut self.sel_edges);
        self.doc.remove_nodes(&nodes);
        self.doc.edges.retain(|e| !edges.contains(&e.id));
        self.finish_edit();
    }

    fn clip_data(&self) -> Option<ClipData> {
        if self.sel_nodes.is_empty() {
            return None;
        }
        let nodes: Vec<Node> = self.doc.nodes.iter().filter(|n| self.sel_nodes.contains(&n.id)).cloned().collect();
        let edges: Vec<Edge> = self
            .doc
            .edges
            .iter()
            .filter(|e| self.sel_nodes.contains(&e.from) && self.sel_nodes.contains(&e.to))
            .cloned()
            .collect();
        Some(ClipData { decisive_tree_clip: 1, nodes, edges })
    }

    fn paste_clip(&mut self, clip: ClipData, offset: Vec2) {
        let mut map = std::collections::HashMap::new();
        self.sel_nodes.clear();
        self.sel_edges.clear();
        for mut n in clip.nodes {
            let id = self.doc.new_id();
            map.insert(n.id, id);
            n.id = id;
            n.pos += offset;
            self.sel_nodes.insert(id);
            self.doc.nodes.push(n);
        }
        for mut e in clip.edges {
            let (Some(&f), Some(&t)) = (map.get(&e.from), map.get(&e.to)) else { continue };
            e.id = self.doc.new_id();
            e.from = f;
            e.to = t;
            self.doc.edges.push(e);
        }
    }

    fn duplicate(&mut self) {
        if let Some(clip) = self.clip_data() {
            self.paste_clip(clip, vec2(30.0, 30.0));
        }
    }

    fn paste_text(&mut self, text: &str) {
        if let Ok(clip) = serde_json::from_str::<ClipData>(text) {
            self.paste_count += 1;
            let k = 20.0 * self.paste_count as f32;
            self.paste_clip(clip, vec2(k, k));
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let p = &PRESETS[self.active_preset];
        let center = self.view().to_world(self.canvas_rect.center());
        let id = self.doc.add_node(center, p.size(), p.style(), text);
        self.select_only_node(id);
    }

    pub fn apply_preset(&mut self, idx: usize) {
        self.active_preset = idx;
        let p = &PRESETS[idx];
        let ids = self.sel_nodes.clone();
        for n in self.doc.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
            let font = n.style.font.clone();
            n.style = p.style();
            n.style.font = font;
        }
    }

    // ---------- View ----------

    pub fn zoom_fit(&mut self) {
        let Some(b) = self.doc.bounds() else {
            self.zoom = 1.0;
            self.pan = vec2(40.0, 40.0);
            return;
        };
        let c = self.canvas_rect;
        if c.width() <= 1.0 {
            self.fit_pending = true;
            return;
        }
        let b = b.expand(30.0);
        self.zoom = (c.width() / b.width()).min(c.height() / b.height()).clamp(MIN_ZOOM, 1.5);
        self.pan = c.center() - c.min - b.center().to_vec2() * self.zoom;
    }

    pub fn zoom_about(&mut self, screen: Pos2, new_zoom: f32) {
        let new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let w = self.view().to_world(screen);
        self.zoom = new_zoom;
        self.pan = screen - self.canvas_rect.min - w.to_vec2() * new_zoom;
    }

    // ---------- Files ----------

    fn file_dialog(&self) -> rfd::FileDialog {
        let mut d = rfd::FileDialog::new().add_filter("Decisive Tree", &[FILE_EXT]);
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        d
    }

    pub fn load(&mut self, path: &Path) {
        let result = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Doc>(&s).map_err(|e| e.to_string()));
        match result {
            Ok(mut doc) => {
                doc.sanitize();
                self.doc = doc;
                self.path = Some(path.to_path_buf());
                self.reset_history();
                self.fit_pending = true;
                self.status = Some((format!("Opened {}", path.display()), 0.0));
            }
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("Could not open file")
                    .set_description(format!("{}\n\n{e}", path.display()))
                    .show();
            }
        }
    }

    fn save_to(&mut self, ctx: &egui::Context, path: PathBuf) -> bool {
        let path = if path.extension().is_none() { path.with_extension(FILE_EXT) } else { path };
        let json = match serde_json::to_string_pretty(&self.doc) {
            Ok(j) => j,
            Err(e) => {
                self.set_status(ctx, format!("Save failed: {e}"));
                return false;
            }
        };
        match std::fs::write(&path, json) {
            Ok(()) => {
                self.saved = self.doc.clone();
                self.set_status(ctx, format!("Saved {}", path.display()));
                self.path = Some(path);
                true
            }
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("Save failed")
                    .set_description(format!("{}\n\n{e}", path.display()))
                    .show();
                false
            }
        }
    }

    fn save(&mut self, ctx: &egui::Context, force_dialog: bool) -> bool {
        self.finish_edit();
        let target = match (&self.path, force_dialog) {
            (Some(p), false) => Some(p.clone()),
            _ => self.file_dialog().set_file_name(self.default_name()).save_file(),
        };
        match target {
            Some(p) => self.save_to(ctx, p),
            None => false,
        }
    }

    fn default_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "flowchart".to_owned())
    }

    fn export(&mut self, ctx: &egui::Context, format: ExportFormat) {
        self.finish_edit();
        if self.doc.nodes.is_empty() {
            self.set_status(ctx, "Nothing to export");
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter(format.ext().to_uppercase(), &[format.ext()])
            .set_file_name(format!("{}.{}", self.default_name(), format.ext()))
            .save_file()
        else {
            return;
        };
        let path = if path.extension().is_none() { path.with_extension(format.ext()) } else { path };
        let routes = routing::compute_routes(&self.doc);
        let svg = export::build_svg(&self.doc, &routes, ctx, &self.fonts);
        match export::write(format, &svg, &path) {
            Ok(()) => self.set_status(ctx, format!("Exported {}", path.display())),
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("Export failed")
                    .set_description(e)
                    .show();
            }
        }
    }

    fn do_pending(&mut self, ctx: &egui::Context, p: Pending) {
        match p {
            Pending::New => {
                self.doc = Doc::default();
                self.path = None;
                self.reset_history();
                self.zoom = 1.0;
                self.pan = vec2(40.0, 40.0);
            }
            Pending::Open => {
                if let Some(path) = self.file_dialog().pick_file() {
                    self.load(&path);
                }
            }
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn guarded(&mut self, ctx: &egui::Context, p: Pending) {
        self.finish_edit();
        if self.dirty() { self.confirm = Some(p) } else { self.do_pending(ctx, p) }
    }

    // ---------- Commands ----------

    pub fn run(&mut self, ctx: &egui::Context, cmd: Cmd) {
        match cmd {
            Cmd::New => self.guarded(ctx, Pending::New),
            Cmd::Open => self.guarded(ctx, Pending::Open),
            Cmd::Quit => self.guarded(ctx, Pending::Quit),
            Cmd::Save => {
                self.save(ctx, false);
            }
            Cmd::SaveAs => {
                self.save(ctx, true);
            }
            Cmd::Export(f) => self.export(ctx, f),
            Cmd::Undo => self.undo(),
            Cmd::Redo => self.redo(),
            Cmd::Copy | Cmd::Cut => {
                if let Some(clip) = self.clip_data() {
                    if let Ok(json) = serde_json::to_string(&clip) {
                        ctx.copy_text(json);
                        self.paste_count = 0;
                    }
                    if cmd == Cmd::Cut {
                        self.delete_selection();
                    }
                }
            }
            Cmd::Paste => {
                self.set_status(ctx, "Use Ctrl+V to paste");
            }
            Cmd::Duplicate => self.duplicate(),
            Cmd::Delete => self.delete_selection(),
            Cmd::SelectAll => {
                self.sel_nodes = self.doc.nodes.iter().map(|n| n.id).collect();
                self.sel_edges = self.doc.edges.iter().map(|e| e.id).collect();
            }
            Cmd::ZoomFit => self.zoom_fit(),
            Cmd::Zoom100 => self.zoom_about(self.canvas_rect.center(), 1.0),
            Cmd::ZoomIn => self.zoom_about(self.canvas_rect.center(), self.zoom * 1.25),
            Cmd::ZoomOut => self.zoom_about(self.canvas_rect.center(), self.zoom / 1.25),
            Cmd::AddChild(d) => {
                if self.editing.is_some() {
                    self.finish_edit();
                }
                self.add_child(d);
            }
            Cmd::EditText => {
                if let Some(id) = self.single_selected_node() {
                    self.start_edit(EditTarget::Node(id));
                } else if self.sel_edges.len() == 1 && self.sel_nodes.is_empty() {
                    let id = *self.sel_edges.iter().next().unwrap();
                    self.start_edit(EditTarget::EdgeLabel(id));
                }
            }
            Cmd::CopyStyle => {
                if let Some(n) = self.doc.nodes.iter().find(|n| self.sel_nodes.contains(&n.id)) {
                    self.style_clipboard = Some(n.style.clone());
                    self.set_status(ctx, "Style copied");
                }
            }
            Cmd::PasteStyle => {
                if let Some(st) = self.style_clipboard.clone() {
                    let ids = self.sel_nodes.clone();
                    for n in self.doc.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
                        n.style = st.clone();
                    }
                }
            }
            Cmd::BringToFront | Cmd::SendToBack => {
                let (sel, rest): (Vec<Node>, Vec<Node>) = std::mem::take(&mut self.doc.nodes).into_iter().partition(|n| self.sel_nodes.contains(&n.id));
                self.doc.nodes = if cmd == Cmd::BringToFront { rest.into_iter().chain(sel).collect() } else { sel.into_iter().chain(rest).collect() };
            }
            Cmd::Nudge(d) => {
                let ids = self.sel_nodes.clone();
                for n in self.doc.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
                    n.pos += d;
                }
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let mut cmds: Vec<Cmd> = Vec::new();

        // Tab is reserved for "add next step" (never focus navigation).
        let tab = self.tab_press.take();
        let editing_canvas_text = self.editing.is_some();
        if let Some(shift) = tab
            && (!typing || editing_canvas_text)
        {
            if let Some(EditTarget::Node(id)) = self.editing {
                self.select_only_node(id);
            }
            cmds.push(Cmd::AddChild(if shift { ChildDir::Down } else { ChildDir::Right }));
        }

        let sc = |m: Modifiers, k: Key| KeyboardShortcut::new(m, k);
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;
        ctx.input_mut(|i| {
            if i.consume_shortcut(&sc(shift_cmd, Key::S)) {
                cmds.push(Cmd::SaveAs);
            }
            if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::S)) {
                cmds.push(Cmd::Save);
            }
            if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::O)) {
                cmds.push(Cmd::Open);
            }
            if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::N)) {
                cmds.push(Cmd::New);
            }
            if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::E)) {
                cmds.push(Cmd::Export(ExportFormat::Png));
            }
        });

        if !typing {
            ctx.input_mut(|i| {
                if i.consume_shortcut(&sc(shift_cmd, Key::Z)) || i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Y)) {
                    cmds.push(Cmd::Redo);
                }
                if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Z)) {
                    cmds.push(Cmd::Undo);
                }
                if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::A)) {
                    cmds.push(Cmd::SelectAll);
                }
                if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::D)) {
                    cmds.push(Cmd::Duplicate);
                }
                if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Num0)) {
                    cmds.push(Cmd::ZoomFit);
                }
                if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Num1)) {
                    cmds.push(Cmd::Zoom100);
                }
                if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Equals)) || i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Plus)) {
                    cmds.push(Cmd::ZoomIn);
                }
                if i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Minus)) {
                    cmds.push(Cmd::ZoomOut);
                }
                if i.consume_shortcut(&sc(shift_cmd, Key::C)) {
                    cmds.push(Cmd::CopyStyle);
                }
                if i.consume_shortcut(&sc(shift_cmd, Key::V)) {
                    cmds.push(Cmd::PasteStyle);
                }
                if i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace) {
                    cmds.push(Cmd::Delete);
                }
                if i.consume_key(Modifiers::NONE, Key::F2) || i.consume_key(Modifiers::NONE, Key::Enter) {
                    cmds.push(Cmd::EditText);
                }
                let step = if i.modifiers.shift { GRID * 5.0 } else if self.snap { GRID } else { 1.0 };
                for (k, d) in [
                    (Key::ArrowLeft, vec2(-step, 0.0)),
                    (Key::ArrowRight, vec2(step, 0.0)),
                    (Key::ArrowUp, vec2(0.0, -step)),
                    (Key::ArrowDown, vec2(0.0, step)),
                ] {
                    if i.consume_key(Modifiers::NONE, k) {
                        cmds.push(Cmd::Nudge(d));
                    }
                }
                if i.consume_key(Modifiers::NONE, Key::Escape) {
                    self.sel_nodes.clear();
                    self.sel_edges.clear();
                    self.drag = Drag::None;
                }
                // Clipboard arrives as events rather than key presses.
                let mut paste: Option<String> = None;
                i.events.retain(|e| match e {
                    Event::Copy => {
                        cmds.push(Cmd::Copy);
                        false
                    }
                    Event::Cut => {
                        cmds.push(Cmd::Cut);
                        false
                    }
                    Event::Paste(t) => {
                        paste = Some(t.clone());
                        false
                    }
                    _ => true,
                });
                if let Some(t) = paste {
                    self.paste_text(&t);
                }
            });
        }
        for c in cmds {
            self.run(ctx, c);
        }
    }

    // ---------- Panels ----------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut cmd: Option<Cmd> = None;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                for (label, c, key) in [
                    ("New", Cmd::New, "Ctrl+N"),
                    ("Open…", Cmd::Open, "Ctrl+O"),
                    ("Save", Cmd::Save, "Ctrl+S"),
                    ("Save As…", Cmd::SaveAs, "Ctrl+Shift+S"),
                ] {
                    if ui.add(egui::Button::new(label).shortcut_text(key)).clicked() {
                        cmd = Some(c);
                    }
                }
                ui.separator();
                for (label, f) in [("Export PNG…", ExportFormat::Png), ("Export PDF…", ExportFormat::Pdf), ("Export SVG…", ExportFormat::Svg)] {
                    if ui.button(label).clicked() {
                        cmd = Some(Cmd::Export(f));
                    }
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    cmd = Some(Cmd::Quit);
                }
            });
            ui.menu_button("Edit", |ui| {
                for (label, c, key) in [
                    ("Undo", Cmd::Undo, "Ctrl+Z"),
                    ("Redo", Cmd::Redo, "Ctrl+Y"),
                    ("Cut", Cmd::Cut, "Ctrl+X"),
                    ("Copy", Cmd::Copy, "Ctrl+C"),
                    ("Duplicate", Cmd::Duplicate, "Ctrl+D"),
                    ("Delete", Cmd::Delete, "Del"),
                    ("Select All", Cmd::SelectAll, "Ctrl+A"),
                    ("Copy Style", Cmd::CopyStyle, "Ctrl+Shift+C"),
                    ("Paste Style", Cmd::PasteStyle, "Ctrl+Shift+V"),
                    ("Bring to Front", Cmd::BringToFront, ""),
                    ("Send to Back", Cmd::SendToBack, ""),
                ] {
                    if ui.add(egui::Button::new(label).shortcut_text(key)).clicked() {
                        cmd = Some(c);
                    }
                }
            });
            ui.menu_button("View", |ui| {
                for (label, c, key) in [
                    ("Zoom to Fit", Cmd::ZoomFit, "Ctrl+0"),
                    ("Zoom 100%", Cmd::Zoom100, "Ctrl+1"),
                    ("Zoom In", Cmd::ZoomIn, "Ctrl++"),
                    ("Zoom Out", Cmd::ZoomOut, "Ctrl+-"),
                ] {
                    if ui.add(egui::Button::new(label).shortcut_text(key)).clicked() {
                        cmd = Some(c);
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.show_grid, "Show grid");
                ui.checkbox(&mut self.snap, "Snap to grid");
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Shortcuts & tips").clicked() {
                    self.show_help = true;
                }
            });
        });
        ui.horizontal(|ui| {
            let b = |ui: &mut egui::Ui, t: &str, tip: &str| ui.button(t).on_hover_text(tip).clicked();
            if b(ui, "🗋 New", "Ctrl+N") {
                cmd = Some(Cmd::New);
            }
            if b(ui, "📂 Open", "Ctrl+O") {
                cmd = Some(Cmd::Open);
            }
            if b(ui, "💾 Save", "Ctrl+S") {
                cmd = Some(Cmd::Save);
            }
            ui.separator();
            if ui.add_enabled(!self.undo.is_empty() || self.doc != self.committed, egui::Button::new("⟲ Undo")).on_hover_text("Ctrl+Z").clicked() {
                cmd = Some(Cmd::Undo);
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("⟳ Redo")).on_hover_text("Ctrl+Y").clicked() {
                cmd = Some(Cmd::Redo);
            }
            ui.separator();
            ui.label("Export:");
            for (t, f) in [("PNG", ExportFormat::Png), ("PDF", ExportFormat::Pdf), ("SVG", ExportFormat::Svg)] {
                if ui.button(t).clicked() {
                    cmd = Some(Cmd::Export(f));
                }
            }
            ui.separator();
            if b(ui, "⛶ Fit", "Zoom to fit (Ctrl+0)") {
                cmd = Some(Cmd::ZoomFit);
            }
            if b(ui, "−", "Zoom out") {
                cmd = Some(Cmd::ZoomOut);
            }
            ui.label(format!("{:.0}%", self.zoom * 100.0));
            if b(ui, "+", "Zoom in") {
                cmd = Some(Cmd::ZoomIn);
            }
            ui.separator();
            ui.checkbox(&mut self.snap, "Snap");
            ui.checkbox(&mut self.show_grid, "Grid");
        });
        if let Some(c) = cmd {
            self.run(&ctx, c);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        ui.horizontal(|ui| {
            let hint = match (&self.drag, self.editing) {
                (_, Some(_)) => "Typing: Enter = done · Shift+Enter = new line · Tab = add next step · Esc = done",
                (Drag::Connect { .. }, _) => "Drop on a node (or one of its ports) to connect · drop on empty space to create a new connected step",
                (Drag::Reconnect { .. }, _) => "Drop on another node or port to re-route this connector",
                (Drag::Bend { .. }, _) => "Align the bend with other connectors or node midpoints (hold Alt to place freely)",
                (Drag::Label { .. }, _) => "Slide the label along the line · snaps to segment midpoints (hold Alt to place freely)",
                _ => "Double-click: add/edit · Drag a blue port dot to connect · Tab: next step · Shift+Tab: branch below · Middle-drag/Space-drag: pan · Ctrl+wheel: zoom",
            };
            ui.label(egui::RichText::new(hint).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{} nodes · {} connectors · {:.0}%", self.doc.nodes.len(), self.doc.edges.len(), self.zoom * 100.0));
                if let Some((msg, t)) = &self.status {
                    if now - *t < 5.0 || *t == 0.0 {
                        ui.separator();
                        ui.label(egui::RichText::new(msg).color(Color32::from_rgb(0x1B, 0x7F, 0x3B)));
                    }
                }
            });
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Shapes");
            ui.label(egui::RichText::new("Click to apply to selection and use for new steps").weak().small());
            let mut clicked = None;
            egui::Grid::new("presets").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
                for (i, p) in PRESETS.iter().enumerate() {
                    if preset_button(ui, p, i == self.active_preset, &self.fonts).clicked() {
                        clicked = Some(i);
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            if let Some(i) = clicked {
                self.apply_preset(i);
            }
            ui.separator();

            if !self.sel_nodes.is_empty() {
                self.node_props(ui, &ctx);
                ui.separator();
            }
            if !self.sel_edges.is_empty() {
                self.edge_props(ui);
                ui.separator();
            }
            if self.sel_nodes.is_empty() && self.sel_edges.is_empty() {
                ui.heading("Quick start");
                ui.label("• Double-click empty canvas to add a step.");
                ui.label("• Hover a node and drag one of its blue port dots onto another node to connect them. Drop on empty space to create a new connected step.");
                ui.label("• Select a node and press Tab to add the next step, Shift+Tab to branch below.");
                ui.label("• Click a connector, then drag its end dots to re-route, or drag its square to move the bend.");
                ui.label("• Drag on empty space to box-select. Shift+click adds to the selection.");
            }
        });
    }

    fn node_props(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let count = self.sel_nodes.len();
        ui.heading(if count == 1 { "Node".to_owned() } else { format!("{count} nodes") });
        let Some(first) = self.doc.nodes.iter().find(|n| self.sel_nodes.contains(&n.id)).cloned() else { return };
        let orig = first.style.clone();
        let mut s = orig.clone();
        let mut size = first.size;

        if count == 1 {
            let mut text = first.text.clone();
            ui.label("Text");
            if ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2).desired_width(f32::INFINITY)).changed()
                && let Some(n) = self.doc.node_mut(first.id)
            {
                n.text = text;
            }
        }

        egui::Grid::new("node_props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Shape");
            egui::ComboBox::from_id_salt("shape").selected_text(s.shape.label()).show_ui(ui, |ui| {
                for sh in NodeShape::ALL {
                    ui.selectable_value(&mut s.shape, sh, sh.label());
                }
            });
            ui.end_row();
            ui.label("Fill");
            ui.color_edit_button_srgba(&mut s.fill);
            ui.end_row();
            ui.label("Border");
            ui.horizontal(|ui| {
                ui.color_edit_button_srgba(&mut s.stroke);
                ui.add(egui::DragValue::new(&mut s.stroke_width).range(0.0..=10.0).speed(0.1).suffix(" px"));
            });
            ui.end_row();
            ui.label("Text color");
            ui.color_edit_button_srgba(&mut s.text_color);
            ui.end_row();
            ui.label("Font");
            egui::ComboBox::from_id_salt("font").selected_text(s.font.clone()).width(150.0).show_ui(ui, |ui| {
                for f in &self.fonts.names {
                    ui.selectable_value(&mut s.font, f.clone(), f);
                }
            });
            ui.end_row();
            ui.label("Font size");
            ui.add(egui::DragValue::new(&mut s.font_size).range(4.0..=96.0).speed(0.25).suffix(" pt"));
            ui.end_row();
            ui.label("Style");
            ui.horizontal(|ui| {
                ui.toggle_value(&mut s.bold, egui::RichText::new("B").strong());
                ui.toggle_value(&mut s.underline, egui::RichText::new("U").underline());
                ui.checkbox(&mut s.shadow, "Shadow");
            });
            ui.end_row();
            ui.label("Size");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut size.x).range(10.0..=2000.0).prefix("W ").speed(1.0));
                ui.add(egui::DragValue::new(&mut size.y).range(10.0..=2000.0).prefix("H ").speed(1.0));
            });
            ui.end_row();
        });

        let ids = self.sel_nodes.clone();
        for n in self.doc.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
            let t = &mut n.style;
            if s.shape != orig.shape {
                t.shape = s.shape;
            }
            if s.fill != orig.fill {
                t.fill = s.fill;
            }
            if s.stroke != orig.stroke {
                t.stroke = s.stroke;
            }
            if s.stroke_width != orig.stroke_width {
                t.stroke_width = s.stroke_width;
            }
            if s.text_color != orig.text_color {
                t.text_color = s.text_color;
            }
            if s.font != orig.font {
                t.font = s.font.clone();
            }
            if s.font_size != orig.font_size {
                t.font_size = s.font_size;
            }
            if s.bold != orig.bold {
                t.bold = s.bold;
            }
            if s.underline != orig.underline {
                t.underline = s.underline;
            }
            if s.shadow != orig.shadow {
                t.shadow = s.shadow;
            }
            if size.x != first.size.x {
                n.size.x = size.x;
            }
            if size.y != first.size.y {
                n.size.y = size.y;
            }
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button("Copy style").clicked() {
                self.run(ctx, Cmd::CopyStyle);
            }
            if ui.add_enabled(self.style_clipboard.is_some(), egui::Button::new("Paste style")).clicked() {
                self.run(ctx, Cmd::PasteStyle);
            }
            if ui.button("To front").clicked() {
                self.run(ctx, Cmd::BringToFront);
            }
            if ui.button("To back").clicked() {
                self.run(ctx, Cmd::SendToBack);
            }
        });

        if count >= 2 {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Arrange").strong());
            ui.horizontal_wrapped(|ui| {
                for (label, a) in [
                    ("Left", Align::Left),
                    ("Center", Align::CenterX),
                    ("Right", Align::Right),
                    ("Top", Align::Top),
                    ("Middle", Align::CenterY),
                    ("Bottom", Align::Bottom),
                ] {
                    if ui.button(label).clicked() {
                        self.align(a);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Distribute ↔").clicked() {
                    self.distribute(true);
                }
                if ui.button("Distribute ↕").clicked() {
                    self.distribute(false);
                }
                if ui.button("Same size").clicked() {
                    let ids = self.sel_nodes.clone();
                    for n in self.doc.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
                        n.size = first.size;
                    }
                }
            });
        }
    }

    fn edge_props(&mut self, ui: &mut egui::Ui) {
        let count = self.sel_edges.len();
        ui.heading(if count == 1 { "Connector".to_owned() } else { format!("{count} connectors") });
        let Some(first) = self.doc.edges.iter().find(|e| self.sel_edges.contains(&e.id)).cloned() else { return };
        let mut e = first.clone();
        egui::Grid::new("edge_props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Label");
            ui.add(egui::TextEdit::singleline(&mut e.label).desired_width(150.0).hint_text("e.g. Yes / No"));
            ui.end_row();
            ui.label("Color");
            ui.color_edit_button_srgba(&mut e.style.color);
            ui.end_row();
            ui.label("Width");
            ui.add(egui::DragValue::new(&mut e.style.width).range(0.5..=8.0).speed(0.1).suffix(" px"));
            ui.end_row();
            ui.label("Line");
            ui.horizontal(|ui| {
                ui.checkbox(&mut e.style.dashed, "Dashed");
                ui.checkbox(&mut e.style.arrow, "Arrow");
            });
            ui.end_row();
            ui.label("Label size");
            ui.add(egui::DragValue::new(&mut e.style.font_size).range(4.0..=48.0).speed(0.25).suffix(" pt"));
            ui.end_row();
            ui.label("Leaves from");
            side_combo(ui, "from_side", &mut e.from_side);
            ui.end_row();
            ui.label("Enters at");
            side_combo(ui, "to_side", &mut e.to_side);
            ui.end_row();
        });
        let ids = self.sel_edges.clone();
        for t in self.doc.edges.iter_mut().filter(|t| ids.contains(&t.id)) {
            if e.label != first.label {
                t.label = e.label.clone();
            }
            if e.style.color != first.style.color {
                t.style.color = e.style.color;
            }
            if e.style.width != first.style.width {
                t.style.width = e.style.width;
            }
            if e.style.dashed != first.style.dashed {
                t.style.dashed = e.style.dashed;
            }
            if e.style.arrow != first.style.arrow {
                t.style.arrow = e.style.arrow;
            }
            if e.style.font_size != first.style.font_size {
                t.style.font_size = e.style.font_size;
            }
            if e.from_side != first.from_side {
                t.from_side = e.from_side;
                t.bend = 0.0;
            }
            if e.to_side != first.to_side {
                t.to_side = e.to_side;
                t.bend = 0.0;
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Straighten (reset bend)").clicked() {
                for t in self.doc.edges.iter_mut().filter(|t| ids.contains(&t.id)) {
                    t.bend = 0.0;
                }
            }
            if ui.button("Auto sides").clicked() {
                for t in self.doc.edges.iter_mut().filter(|t| ids.contains(&t.id)) {
                    t.from_side = None;
                    t.to_side = None;
                    t.bend = 0.0;
                }
            }
            if ui.button("Reverse direction").clicked() {
                for t in self.doc.edges.iter_mut().filter(|t| ids.contains(&t.id)) {
                    std::mem::swap(&mut t.from, &mut t.to);
                    std::mem::swap(&mut t.from_side, &mut t.to_side);
                    t.bend = 0.0;
                    t.label_pos = match t.label_pos {
                        LabelPos::Along(f) => LabelPos::Along(1.0 - f),
                        _ => LabelPos::Auto,
                    };
                }
            }
            if ui.button("Auto label position").on_hover_text("Labels can be dragged along their connector").clicked() {
                for t in self.doc.edges.iter_mut().filter(|t| ids.contains(&t.id)) {
                    t.label_pos = LabelPos::Auto;
                }
            }
        });
    }

    fn align(&mut self, a: Align) {
        let rects: Vec<Rect> = self.doc.nodes.iter().filter(|n| self.sel_nodes.contains(&n.id)).map(|n| n.rect()).collect();
        let Some(b) = rects.iter().copied().reduce(|x, y| x.union(y)) else { return };
        let ids = self.sel_nodes.clone();
        for n in self.doc.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
            match a {
                Align::Left => n.pos.x = b.min.x,
                Align::Right => n.pos.x = b.max.x - n.size.x,
                Align::CenterX => n.pos.x = b.center().x - n.size.x * 0.5,
                Align::Top => n.pos.y = b.min.y,
                Align::Bottom => n.pos.y = b.max.y - n.size.y,
                Align::CenterY => n.pos.y = b.center().y - n.size.y * 0.5,
            }
        }
    }

    fn distribute(&mut self, horizontal: bool) {
        let mut sel: Vec<(u64, Rect)> = self.doc.nodes.iter().filter(|n| self.sel_nodes.contains(&n.id)).map(|n| (n.id, n.rect())).collect();
        if sel.len() < 3 {
            return;
        }
        let key = |r: &Rect| if horizontal { r.center().x } else { r.center().y };
        sel.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
        let first = key(&sel[0].1);
        let last = key(&sel[sel.len() - 1].1);
        let step = (last - first) / (sel.len() - 1) as f32;
        for (i, (id, _)) in sel.iter().enumerate() {
            let c = first + step * i as f32;
            if let Some(n) = self.doc.node_mut(*id) {
                if horizontal {
                    n.pos.x = c - n.size.x * 0.5;
                } else {
                    n.pos.y = c - n.size.y * 0.5;
                }
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(p) = self.confirm {
            let mut choice: Option<u8> = None;
            egui::Modal::new(egui::Id::new("confirm_unsaved")).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.heading("Unsaved changes");
                ui.label("Save changes to this flowchart before continuing?");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Don't save").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(2);
                    }
                });
            });
            match choice {
                Some(0) => {
                    self.confirm = None;
                    if self.save(ctx, false) {
                        self.do_pending(ctx, p);
                    }
                }
                Some(1) => {
                    self.confirm = None;
                    self.do_pending(ctx, p);
                }
                Some(2) => self.confirm = None,
                _ => {}
            }
        }
        if self.show_help {
            egui::Window::new("Shortcuts & tips").open(&mut self.show_help).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("help").num_columns(2).striped(true).show(ui, |ui| {
                    for (k, v) in HELP {
                        ui.label(egui::RichText::new(*k).strong());
                        ui.label(*v);
                        ui.end_row();
                    }
                });
            });
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        let title = format!("{}{} — Decisive Tree", name, if self.dirty() { " •" } else { "" });
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn handle_os_events(&mut self, ctx: &egui::Context) {
        let dropped: Option<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| Some(f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty())));
        if let Some(p) = dropped {
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(FILE_EXT)) {
                if self.dirty() {
                    let open = rfd::MessageDialog::new()
                        .set_title("Unsaved changes")
                        .set_description("Discard unsaved changes and open the dropped file?")
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show();
                    if open == rfd::MessageDialogResult::Yes {
                        self.load(&p);
                    }
                } else {
                    self.load(&p);
                }
            } else {
                self.set_status(ctx, format!("Only .{FILE_EXT} files can be opened"));
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.finish_edit();
            self.confirm = Some(Pending::Quit);
        }
    }
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
    CenterX,
    Top,
    Bottom,
    CenterY,
}

const HELP: &[(&str, &str)] = &[
    ("Double-click canvas", "Add a step (uses the active shape)"),
    ("Double-click node", "Edit its text"),
    ("Double-click connector", "Edit its label"),
    ("Drag ● port", "Connect to another node; drop on empty space to create a connected step"),
    ("Alt + drag node", "Connect from the node (auto side)"),
    ("Tab / Shift+Tab", "Add next step to the right / branch below (also while typing)"),
    ("Enter / F2", "Edit selected node text"),
    ("Enter while typing", "Finish (Shift+Enter for a new line)"),
    ("Drag connector end dot", "Re-route to another node or port"),
    ("Drag connector square", "Move the bend; snaps to connectors and node midpoints (Alt = free)"),
    ("Drag connector label", "Slide it along the line; snaps to segment midpoints (Alt = free)"),
    ("Drag empty space", "Box select (Shift = add)"),
    ("Shift+click", "Add/remove from selection"),
    ("Arrows / Shift+Arrows", "Nudge selection"),
    ("Middle-drag, Space+drag", "Pan"),
    ("Mouse wheel", "Pan (Shift = horizontal)"),
    ("Ctrl + wheel", "Zoom at cursor"),
    ("Ctrl+0 / Ctrl+1", "Zoom to fit / 100%"),
    ("Ctrl+C / X / V / D", "Copy / Cut / Paste / Duplicate (works between windows)"),
    ("Ctrl+Shift+C / V", "Copy / paste style"),
    ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
    ("Ctrl+S / Ctrl+O / Ctrl+E", "Save / Open / Export PNG"),
    ("Right-click", "Context menu"),
];

fn side_combo(ui: &mut egui::Ui, id: &str, side: &mut Option<Side>) {
    let text = side.map(|s| s.label()).unwrap_or("Auto");
    egui::ComboBox::from_id_salt(id).selected_text(text).show_ui(ui, |ui| {
        ui.selectable_value(side, None, "Auto");
        for s in Side::ALL {
            ui.selectable_value(side, Some(s), s.label());
        }
    });
}

fn preset_button(ui: &mut egui::Ui, p: &crate::presets::Preset, active: bool, _fonts: &FontRegistry) -> egui::Response {
    let size = vec2(118.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter_at(rect);
    let bg = if active {
        Color32::from_rgb(0xDD, 0xEA, 0xFB)
    } else if resp.hovered() {
        Color32::from_gray(238)
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 4, bg);
    if active {
        painter.rect_stroke(rect, 4, egui::Stroke::new(1.0, crate::render::SELECT_COLOR), egui::StrokeKind::Inside);
    }
    let icon = Rect::from_center_size(rect.left_center() + vec2(18.0, 0.0), vec2(26.0, 18.0));
    let style = p.style();
    let fake = Node { id: 0, pos: icon.min, size: icon.size(), text: String::new(), style };
    let view = View { origin: Pos2::ZERO, pan: Vec2::ZERO, zoom: 1.0 };
    if p.shape == NodeShape::Text {
        painter.text(icon.center(), egui::Align2::CENTER_CENTER, "T", egui::FontId::proportional(16.0), Color32::from_gray(40));
    } else {
        let mut f = fake;
        f.style.shadow = false;
        crate::render::paint_node(&painter, &view, &f, None);
    }
    painter.text(
        rect.left_center() + vec2(36.0, 0.0),
        egui::Align2::LEFT_CENTER,
        p.name,
        egui::FontId::proportional(12.0),
        ui.visuals().text_color(),
    );
    resp.on_hover_text(format!("{} ({})", p.name, p.shape.label()))
}

impl eframe::App for DecisiveApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        raw.events.retain(|e| match e {
            Event::Key { key: Key::Tab, pressed, modifiers, .. } => {
                if *pressed {
                    self.tab_press = Some(modifiers.shift);
                }
                false
            }
            _ => true,
        });
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_os_events(&ctx);
        if self.confirm.is_none() {
            self.handle_shortcuts(&ctx);
        }

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::right("props").resizable(true).default_size(270.0).min_size(230.0).show(ui, |ui| self.side_panel(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| self.canvas(ui));

        self.dialogs(&ctx);
        self.prune_selection();
        self.commit_if_changed(&ctx);
        self.update_title(&ctx);
        if let Some((_, t)) = self.status
            && t > 0.0
        {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(5.1));
        }
    }
}
