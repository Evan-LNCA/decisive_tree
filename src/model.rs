use egui::{Color32, Pos2, Rect, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};

pub const FILE_EXT: &str = "dtree";
pub const FILE_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum NodeShape {
    Rect,
    Rounded,
    Pill,
    Diamond,
    Ellipse,
    Parallelogram,
    Text,
}

impl NodeShape {
    pub const ALL: [NodeShape; 7] = [
        NodeShape::Rect,
        NodeShape::Rounded,
        NodeShape::Pill,
        NodeShape::Diamond,
        NodeShape::Ellipse,
        NodeShape::Parallelogram,
        NodeShape::Text,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NodeShape::Rect => "Rectangle",
            NodeShape::Rounded => "Rounded",
            NodeShape::Pill => "Pill",
            NodeShape::Diamond => "Diamond",
            NodeShape::Ellipse => "Ellipse",
            NodeShape::Parallelogram => "Parallelogram",
            NodeShape::Text => "Text only",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];

    /// Outward unit direction of this side.
    pub fn dir(self) -> Vec2 {
        match self {
            Side::Top => vec2(0.0, -1.0),
            Side::Right => vec2(1.0, 0.0),
            Side::Bottom => vec2(0.0, 1.0),
            Side::Left => vec2(-1.0, 0.0),
        }
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }

    pub fn label(self) -> &'static str {
        match self {
            Side::Top => "Top",
            Side::Right => "Right",
            Side::Bottom => "Bottom",
            Side::Left => "Left",
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct NodeStyle {
    pub shape: NodeShape,
    pub fill: Color32,
    pub stroke: Color32,
    pub stroke_width: f32,
    pub text_color: Color32,
    pub font: String,
    pub font_size: f32,
    pub bold: bool,
    pub underline: bool,
    pub shadow: bool,
}

impl Default for NodeStyle {
    fn default() -> Self {
        crate::presets::PRESETS[0].style()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Node {
    pub id: u64,
    /// Top-left corner in world coordinates.
    pub pos: Pos2,
    pub size: Vec2,
    pub text: String,
    #[serde(default)]
    pub style: NodeStyle,
}

impl Node {
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(self.pos, self.size)
    }

    pub fn center(&self) -> Pos2 {
        self.pos + self.size * 0.5
    }

    pub fn contains(&self, p: Pos2) -> bool {
        let r = self.rect();
        if !r.contains(p) {
            return false;
        }
        match self.style.shape {
            NodeShape::Diamond => {
                let c = r.center();
                let d = p - c;
                d.x.abs() / (r.width() * 0.5) + d.y.abs() / (r.height() * 0.5) <= 1.02
            }
            NodeShape::Ellipse => {
                let c = r.center();
                let d = p - c;
                let a = r.width() * 0.5;
                let b = r.height() * 0.5;
                (d.x * d.x) / (a * a) + (d.y * d.y) / (b * b) <= 1.02
            }
            _ => true,
        }
    }

    /// World-space point where connectors attach on the given side.
    pub fn port(&self, side: Side) -> Pos2 {
        let r = self.rect();
        let skew = parallelogram_skew(r);
        match (self.style.shape, side) {
            (NodeShape::Parallelogram, Side::Left) => pos2(r.min.x + skew * 0.5, r.center().y),
            (NodeShape::Parallelogram, Side::Right) => pos2(r.max.x - skew * 0.5, r.center().y),
            (_, Side::Top) => pos2(r.center().x, r.min.y),
            (_, Side::Bottom) => pos2(r.center().x, r.max.y),
            (_, Side::Left) => pos2(r.min.x, r.center().y),
            (_, Side::Right) => pos2(r.max.x, r.center().y),
        }
    }
}

pub fn parallelogram_skew(r: Rect) -> f32 {
    (r.height() * 0.35).min(r.width() * 0.25)
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct EdgeStyle {
    pub color: Color32,
    pub width: f32,
    pub dashed: bool,
    pub arrow: bool,
    pub label_color: Color32,
    pub font_size: f32,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            color: Color32::from_rgb(0x4A, 0x50, 0x5C),
            width: 1.5,
            dashed: false,
            arrow: true,
            label_color: Color32::from_rgb(0x22, 0x22, 0x22),
            font_size: 10.0,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Edge {
    pub id: u64,
    pub from: u64,
    pub to: u64,
    /// `None` = choose automatically from relative node positions.
    #[serde(default)]
    pub from_side: Option<Side>,
    #[serde(default)]
    pub to_side: Option<Side>,
    #[serde(default)]
    pub label: String,
    /// User offset applied to the draggable middle segment of the route.
    #[serde(default)]
    pub bend: f32,
    #[serde(default)]
    pub style: EdgeStyle,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Doc {
    pub version: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub next_id: u64,
}

impl Default for Doc {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            nodes: Vec::new(),
            edges: Vec::new(),
            next_id: 1,
        }
    }
}

impl Doc {
    pub fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn node(&self, id: u64) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: u64) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn edge(&self, id: u64) -> Option<&Edge> {
        self.edges.iter().find(|e| e.id == id)
    }

    pub fn edge_mut(&mut self, id: u64) -> Option<&mut Edge> {
        self.edges.iter_mut().find(|e| e.id == id)
    }

    pub fn add_node(&mut self, center: Pos2, size: Vec2, style: NodeStyle, text: &str) -> u64 {
        let id = self.new_id();
        self.nodes.push(Node {
            id,
            pos: center - size * 0.5,
            size,
            text: text.to_owned(),
            style,
        });
        id
    }

    pub fn add_edge(&mut self, from: u64, to: u64, from_side: Option<Side>, to_side: Option<Side>) -> Option<u64> {
        if from == to {
            return None;
        }
        if let Some(existing) = self.edges.iter().find(|e| e.from == from && e.to == to) {
            return Some(existing.id);
        }
        let id = self.new_id();
        self.edges.push(Edge {
            id,
            from,
            to,
            from_side,
            to_side,
            label: String::new(),
            bend: 0.0,
            style: EdgeStyle::default(),
        });
        Some(id)
    }

    /// Removes nodes and every edge touching them.
    pub fn remove_nodes(&mut self, ids: &std::collections::HashSet<u64>) {
        self.nodes.retain(|n| !ids.contains(&n.id));
        self.edges.retain(|e| !ids.contains(&e.from) && !ids.contains(&e.to));
    }

    pub fn bounds(&self) -> Option<Rect> {
        let mut it = self.nodes.iter();
        let first = it.next()?.rect();
        Some(it.fold(first, |acc, n| acc.union(n.rect())))
    }

    /// Ensures ids are consistent after loading a file from disk.
    pub fn sanitize(&mut self) {
        let max_id = self
            .nodes
            .iter()
            .map(|n| n.id)
            .chain(self.edges.iter().map(|e| e.id))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(max_id + 1);
        let ids: std::collections::HashSet<u64> = self.nodes.iter().map(|n| n.id).collect();
        self.edges.retain(|e| ids.contains(&e.from) && ids.contains(&e.to) && e.from != e.to);
        for n in &mut self.nodes {
            n.size.x = n.size.x.max(10.0);
            n.size.y = n.size.y.max(10.0);
            n.style.font_size = n.style.font_size.clamp(4.0, 200.0);
        }
    }
}
