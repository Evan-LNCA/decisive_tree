use egui::{FontData, FontDefinitions, FontFamily};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

pub const SANS: &str = "Sans (built-in)";
pub const MONO: &str = "Mono (built-in)";

/// (display name, regular file, bold file) inside the Windows fonts directory.
const SYSTEM_FONTS: &[(&str, &str, &str)] = &[
    ("Arial", "arial.ttf", "arialbd.ttf"),
    ("Segoe UI", "segoeui.ttf", "segoeuib.ttf"),
    ("Calibri", "calibri.ttf", "calibrib.ttf"),
    ("Verdana", "verdana.ttf", "verdanab.ttf"),
    ("Tahoma", "tahoma.ttf", "tahomabd.ttf"),
    ("Trebuchet MS", "trebuc.ttf", "trebucbd.ttf"),
    ("Times New Roman", "times.ttf", "timesbd.ttf"),
    ("Georgia", "georgia.ttf", "georgiab.ttf"),
    ("Consolas", "consola.ttf", "consolab.ttf"),
    ("Courier New", "cour.ttf", "courbd.ttf"),
];

/// Fonts that were successfully registered with egui.
#[derive(Default)]
pub struct FontRegistry {
    pub names: Vec<String>,
    with_bold: BTreeSet<String>,
}

impl FontRegistry {
    pub fn family(&self, name: &str, bold: bool) -> FontFamily {
        if self.names.iter().any(|n| n == name) && name != SANS && name != MONO {
            if bold && self.with_bold.contains(name) {
                return FontFamily::Name(format!("{name}#bold").into());
            }
            return FontFamily::Name(name.into());
        }
        if name == MONO { FontFamily::Monospace } else { FontFamily::Proportional }
    }

    /// Name used for SVG export (falls back to generic CSS families).
    pub fn css_family(&self, name: &str) -> String {
        match name {
            SANS => "Arial, Helvetica, sans-serif".to_owned(),
            MONO => "Consolas, 'Courier New', monospace".to_owned(),
            other => format!("'{other}', Arial, sans-serif"),
        }
    }
}

fn fonts_dir() -> Option<PathBuf> {
    let windir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot"))?;
    Some(PathBuf::from(windir).join("Fonts"))
}

pub fn install(ctx: &egui::Context) -> FontRegistry {
    let mut defs = FontDefinitions::default();
    let fallbacks: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut reg = FontRegistry::default();

    if let Some(dir) = fonts_dir() {
        for (name, regular, bold) in SYSTEM_FONTS {
            let Ok(bytes) = std::fs::read(dir.join(regular)) else { continue };
            defs.font_data.insert(name.to_string(), Arc::new(FontData::from_owned(bytes)));
            let mut fam = vec![name.to_string()];
            fam.extend(fallbacks.iter().cloned());
            defs.families.insert(FontFamily::Name((*name).into()), fam);
            reg.names.push(name.to_string());

            if let Ok(bytes) = std::fs::read(dir.join(bold)) {
                let key = format!("{name}#bold");
                defs.font_data.insert(key.clone(), Arc::new(FontData::from_owned(bytes)));
                let mut fam = vec![key.clone()];
                fam.extend(fallbacks.iter().cloned());
                defs.families.insert(FontFamily::Name(key.into()), fam);
                reg.with_bold.insert(name.to_string());
            }
        }
    }
    reg.names.push(SANS.to_owned());
    reg.names.push(MONO.to_owned());
    ctx.set_fonts(defs);
    reg
}
