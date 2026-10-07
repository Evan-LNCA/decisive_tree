use crate::model::{NodeShape, NodeStyle};
use egui::{Color32, Vec2, vec2};

/// Style presets sampled from the reference "Redtag Process" flowchart.
pub struct Preset {
    pub name: &'static str,
    pub shape: NodeShape,
    pub fill: [u8; 3],
    pub size: [f32; 2],
    pub bold: bool,
    pub font_size: f32,
}

const BORDER: [u8; 3] = [0x3A, 0x3A, 0x3A];
const TEXT: [u8; 3] = [0x1E, 0x1E, 0x1E];
pub const DEFAULT_FONT: &str = "Arial";

pub const PRESETS: [Preset; 11] = [
    Preset { name: "Process", shape: NodeShape::Rect, fill: [0xFF, 0xF8, 0xC4], size: [100.0, 60.0], bold: false, font_size: 11.0 },
    Preset { name: "Data / Reading", shape: NodeShape::Rounded, fill: [0xCF, 0xE3, 0xFA], size: [110.0, 40.0], bold: false, font_size: 11.0 },
    Preset { name: "Record / Action", shape: NodeShape::Pill, fill: [0xB2, 0xF0, 0xE6], size: [100.0, 40.0], bold: false, font_size: 11.0 },
    Preset { name: "Sub-step", shape: NodeShape::Pill, fill: [0xF3, 0xE6, 0xFA], size: [100.0, 46.0], bold: false, font_size: 11.0 },
    Preset { name: "Decision", shape: NodeShape::Diamond, fill: [0xFF, 0xD3, 0x9B], size: [90.0, 70.0], bold: false, font_size: 11.0 },
    Preset { name: "Fail Outcome", shape: NodeShape::Rect, fill: [0xFF, 0xD0, 0xD0], size: [100.0, 60.0], bold: false, font_size: 11.0 },
    Preset { name: "Pass Outcome", shape: NodeShape::Rect, fill: [0xE1, 0xF7, 0xDA], size: [100.0, 60.0], bold: false, font_size: 11.0 },
    Preset { name: "Yes", shape: NodeShape::Pill, fill: [0xD6, 0xF5, 0xD0], size: [50.0, 32.0], bold: false, font_size: 11.0 },
    Preset { name: "No", shape: NodeShape::Pill, fill: [0xFF, 0xCF, 0xCF], size: [50.0, 32.0], bold: false, font_size: 11.0 },
    Preset { name: "Input / Output", shape: NodeShape::Parallelogram, fill: [0xE4, 0xEC, 0xF7], size: [110.0, 50.0], bold: false, font_size: 11.0 },
    Preset { name: "Header Text", shape: NodeShape::Text, fill: [0xFF, 0xFF, 0xFF], size: [120.0, 30.0], bold: true, font_size: 18.0 },
];

impl Preset {
    pub fn style(&self) -> NodeStyle {
        let [r, g, b] = self.fill;
        let fill = if self.shape == NodeShape::Text { Color32::TRANSPARENT } else { Color32::from_rgb(r, g, b) };
        NodeStyle {
            shape: self.shape,
            fill,
            stroke: Color32::from_rgb(BORDER[0], BORDER[1], BORDER[2]),
            stroke_width: if self.shape == NodeShape::Text { 0.0 } else { 1.0 },
            text_color: Color32::from_rgb(TEXT[0], TEXT[1], TEXT[2]),
            font: DEFAULT_FONT.to_owned(),
            font_size: self.font_size,
            bold: self.bold,
            underline: self.shape == NodeShape::Text,
            shadow: matches!(self.shape, NodeShape::Rect),
        }
    }

    pub fn size(&self) -> Vec2 {
        vec2(self.size[0], self.size[1])
    }

    pub fn swatch(&self) -> Color32 {
        let [r, g, b] = self.fill;
        Color32::from_rgb(r, g, b)
    }
}
