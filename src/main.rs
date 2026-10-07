#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
mod export;
mod fonts;
mod model;
mod presets;
mod render;
mod routing;

use std::path::PathBuf;

const WGPU_FLAG: &str = "--renderer=wgpu";

fn app_icon() -> egui::IconData {
    // Orange decision diamond on a yellow process box, generated at startup.
    let n = 64usize;
    let mut rgba = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let i = (y * n + x) * 4;
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let c = (fx - 32.0).abs() + (fy - 32.0).abs();
            let px: Option<[u8; 4]> = if c <= 28.0 {
                Some(if c >= 25.5 { [0x40, 0x40, 0x40, 255] } else { [0xF4, 0xB1, 0x83, 255] })
            } else {
                None
            };
            if let Some(p) = px {
                rgba[i..i + 4].copy_from_slice(&p);
            }
        }
    }
    egui::IconData { rgba, width: n as u32, height: n as u32 }
}

fn options(wgpu: bool) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Decisive Tree")
            .with_inner_size([1400.0, 850.0])
            .with_min_inner_size([700.0, 450.0])
            .with_icon(app_icon())
            .with_drag_and_drop(true),
        renderer: if wgpu { eframe::Renderer::Wgpu } else { eframe::Renderer::Glow },
        ..Default::default()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wgpu = args.iter().any(|a| a == WGPU_FLAG);
    let file = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from);

    let f = file.clone();
    let result = eframe::run_native(
        "Decisive Tree",
        options(wgpu),
        Box::new(move |cc| Ok(Box::new(app::DecisiveApp::new(cc, f)))),
    );
    if let Err(e) = result {
        if !wgpu {
            // winit cannot recreate its event loop in-process, so retry with DirectX/Vulkan (wgpu) in a fresh process.
            if let Ok(exe) = std::env::current_exe() {
                let mut cmd = std::process::Command::new(exe);
                cmd.args(&args).arg(WGPU_FLAG);
                if cmd.status().is_ok() {
                    return;
                }
            }
        }
        rfd::MessageDialog::new()
            .set_title("Decisive Tree")
            .set_description(format!("Could not start the graphics renderer:\n{e}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
