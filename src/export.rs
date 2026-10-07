use crate::fonts::FontRegistry;
use crate::model::{Doc, NodeShape};
use crate::render::{ARROW_SIZE, arrow_head, corner_radius, layout_plain, layout_text, shape_polygon, text_wrap_width};
use crate::routing::{Route, label_anchor};
use egui::{Color32, FontId, Pos2, Rect, vec2};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

const MARGIN: f32 = 30.0;
const PNG_SCALE: f32 = 3.0;
const MAX_PNG_DIM: f32 = 16000.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExportFormat {
    Png,
    Svg,
    Pdf,
}

impl ExportFormat {
    pub fn ext(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
        }
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn paint(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 0 {
        "none".to_owned()
    } else if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}\" fill-opacity=\"{:.3}\" stroke-opacity=\"{:.3}", a as f32 / 255.0, a as f32 / 255.0)
    }
}

fn pts_attr(points: &[Pos2]) -> String {
    let mut s = String::new();
    for p in points {
        let _ = write!(s, "{:.2},{:.2} ", p.x, p.y);
    }
    s
}

fn shape_svg(out: &mut String, rect: Rect, shape: NodeShape, fill: &str, stroke: &str, stroke_width: f32) {
    let stroke_attr = if stroke_width > 0.0 {
        format!("stroke=\"{stroke}\" stroke-width=\"{stroke_width:.2}\"")
    } else {
        "stroke=\"none\"".to_owned()
    };
    match shape {
        NodeShape::Ellipse => {
            let c = rect.center();
            let _ = writeln!(
                out,
                "<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" fill=\"{fill}\" {stroke_attr}/>",
                c.x,
                c.y,
                rect.width() * 0.5,
                rect.height() * 0.5
            );
        }
        NodeShape::Diamond | NodeShape::Parallelogram => {
            let poly = shape_polygon(rect, shape).unwrap_or_default();
            let _ = writeln!(out, "<polygon points=\"{}\" fill=\"{fill}\" {stroke_attr} stroke-linejoin=\"miter\"/>", pts_attr(&poly));
        }
        _ => {
            let r = corner_radius(shape, rect.size());
            let _ = writeln!(
                out,
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{r:.2}\" fill=\"{fill}\" {stroke_attr}/>",
                rect.min.x,
                rect.min.y,
                rect.width(),
                rect.height()
            );
        }
    }
}

/// Emits wrapped text using egui's layout so line breaks match the editor.
fn text_svg(out: &mut String, galley: &egui::Galley, center: Pos2, family: &str, size: f32, bold: bool, underline: bool, color: Color32) {
    let origin = center - galley.rect.center().to_vec2();
    let weight = if bold { " font-weight=\"bold\"" } else { "" };
    let deco = if underline { " text-decoration=\"underline\"" } else { "" };
    for placed in &galley.rows {
        let text: String = placed.row.glyphs.iter().map(|g| g.chr).collect();
        let text = text.trim_end_matches(['\n', '\r']);
        if text.trim().is_empty() {
            continue;
        }
        let baseline = placed.row.glyphs.first().map(|g| g.pos.y).unwrap_or(size * 0.8);
        let x = origin.x + placed.pos.x + placed.row.size.x * 0.5;
        let y = origin.y + placed.pos.y + baseline;
        let _ = writeln!(
            out,
            "<text x=\"{x:.2}\" y=\"{y:.2}\" font-family=\"{}\" font-size=\"{size:.2}\"{weight}{deco} fill=\"{}\" text-anchor=\"middle\" xml:space=\"preserve\">{}</text>",
            esc(family),
            paint(color),
            esc(text)
        );
    }
}

pub fn build_svg(doc: &Doc, routes: &HashMap<u64, Route>, ctx: &egui::Context, fonts: &FontRegistry) -> String {
    let mut bounds = doc.bounds().unwrap_or(Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)));
    for r in routes.values() {
        for p in &r.points {
            bounds.extend_with(*p);
        }
    }
    let bounds = bounds.expand(MARGIN);
    let mut out = String::with_capacity(4096 + doc.nodes.len() * 600);
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"{:.2} {:.2} {w:.2} {h:.2}\">",
        bounds.min.x,
        bounds.min.y,
        w = bounds.width(),
        h = bounds.height()
    );
    let _ = writeln!(
        out,
        "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"#ffffff\"/>",
        bounds.min.x,
        bounds.min.y,
        bounds.width(),
        bounds.height()
    );

    // Connectors underneath nodes.
    for e in &doc.edges {
        let Some(route) = routes.get(&e.id) else { continue };
        if route.points.len() < 2 {
            continue;
        }
        let mut pts = route.points.clone();
        let n = pts.len();
        let dir = pts[n - 1] - pts[n - 2];
        let tip = pts[n - 1];
        let head = ARROW_SIZE * (e.style.width / 1.5).max(1.0).sqrt();
        if e.style.arrow && dir.length() > head * 1.2 {
            pts[n - 1] = tip - dir.normalized() * head * 1.2;
        }
        let dash = if e.style.dashed { " stroke-dasharray=\"6 4\"" } else { "" };
        let color = paint(e.style.color);
        let _ = writeln!(
            out,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{:.2}\"{dash} stroke-linejoin=\"miter\"/>",
            pts_attr(&pts),
            e.style.width
        );
        if e.style.arrow && dir.length() > 0.1 {
            let _ = writeln!(out, "<polygon points=\"{}\" fill=\"{color}\"/>", pts_attr(&arrow_head(tip, dir, head)));
        }
    }
    for e in &doc.edges {
        if e.label.is_empty() {
            continue;
        }
        let Some(route) = routes.get(&e.id) else { continue };
        let anchor = label_anchor(&route.points);
        let font = FontId::new(e.style.font_size, fonts.family(crate::presets::DEFAULT_FONT, false));
        let g = layout_plain(ctx, &e.label, font, e.style.label_color, false, f32::INFINITY, 1.0);
        let bg = Rect::from_center_size(anchor, g.rect.size() + vec2(6.0, 2.0));
        let _ = writeln!(
            out,
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"2\" fill=\"#ffffff\"/>",
            bg.min.x,
            bg.min.y,
            bg.width(),
            bg.height()
        );
        let family = fonts.css_family(crate::presets::DEFAULT_FONT);
        text_svg(&mut out, &g, anchor, &family, e.style.font_size, false, false, e.style.label_color);
    }

    for n in &doc.nodes {
        let st = &n.style;
        let rect = n.rect();
        if st.shape != NodeShape::Text {
            if st.shadow {
                let fill = "#000000\" fill-opacity=\"0.15";
                shape_svg(&mut out, rect.translate(vec2(2.0, 2.5)), st.shape, fill, "none", 0.0);
            }
            shape_svg(&mut out, rect, st.shape, &paint(st.fill), &paint(st.stroke), st.stroke_width);
        } else if st.fill.a() > 0 {
            shape_svg(&mut out, rect, NodeShape::Rect, &paint(st.fill), "none", 0.0);
        }
        if !n.text.is_empty() {
            let g = layout_text(ctx, fonts, &n.text, st, text_wrap_width(n), 1.0);
            text_svg(&mut out, &g, rect.center(), &fonts.css_family(&st.font), st.font_size, st.bold, st.underline, st.text_color);
        }
    }
    out.push_str("</svg>\n");
    out
}

fn parse_tree(svg: &str) -> Result<resvg::usvg::Tree, String> {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    db.set_sans_serif_family("Arial");
    db.set_monospace_family("Consolas");
    let opt = resvg::usvg::Options { fontdb: Arc::new(db), ..Default::default() };
    resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| format!("SVG parse failed: {e}"))
}

pub fn write(format: ExportFormat, svg: &str, path: &Path) -> Result<(), String> {
    match format {
        ExportFormat::Svg => std::fs::write(path, svg).map_err(|e| format!("Could not write {}: {e}", path.display())),
        ExportFormat::Png => {
            let tree = parse_tree(svg)?;
            let size = tree.size();
            let scale = PNG_SCALE.min(MAX_PNG_DIM / size.width().max(size.height()).max(1.0));
            let w = (size.width() * scale).ceil().max(1.0) as u32;
            let h = (size.height() * scale).ceil().max(1.0) as u32;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or_else(|| format!("Image too large ({w}x{h})"))?;
            pixmap.fill(resvg::tiny_skia::Color::WHITE);
            resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
            pixmap.save_png(path).map_err(|e| format!("Could not write {}: {e}", path.display()))
        }
        ExportFormat::Pdf => {
            let tree = parse_tree(svg)?;
            let pdf = svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default())
                .map_err(|e| format!("PDF conversion failed: {e}"))?;
            std::fs::write(path, pdf).map_err(|e| format!("Could not write {}: {e}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::PRESETS;

    #[test]
    fn exports_all_formats() {
        let ctx = egui::Context::default();
        let fonts = crate::fonts::install(&ctx);
        let mut doc = Doc::default();
        let a = doc.add_node(Pos2::new(0.0, 0.0), PRESETS[0].size(), PRESETS[0].style(), "Start: Unit fails test");
        let q = doc.add_node(Pos2::new(160.0, 0.0), PRESETS[4].size(), PRESETS[4].style(), "Pass?");
        let y = doc.add_node(Pos2::new(300.0, -50.0), PRESETS[6].size(), PRESETS[6].style(), "Deliver");
        let n = doc.add_node(Pos2::new(300.0, 50.0), PRESETS[5].size(), PRESETS[5].style(), "Red Tag");
        doc.add_edge(a, q, None, None);
        let e = doc.add_edge(q, y, None, None).unwrap();
        doc.edge_mut(e).unwrap().label = "Yes".into();
        doc.add_edge(q, n, None, None);
        let mut svg = String::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {
            svg = build_svg(&doc, &crate::routing::compute_routes(&doc), &ctx, &fonts);
        });
        out.textures_delta.clear();
        assert!(svg.contains("Pass?"));
        let dir = std::env::temp_dir();
        for f in [ExportFormat::Png, ExportFormat::Svg, ExportFormat::Pdf] {
            let p = dir.join(format!("dt_export_test.{}", f.ext()));
            write(f, &svg, &p).unwrap();
            assert!(std::fs::metadata(&p).unwrap().len() > 500, "{f:?} too small");
        }
        // Headless context: skip egui's texture-delta drop check.
        std::mem::forget(ctx);
    }
}
