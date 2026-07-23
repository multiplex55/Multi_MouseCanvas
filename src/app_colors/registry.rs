use crate::{capture::foreground::ApplicationIdentity, settings::model::RgbaColor};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationColorMode {
    FixedGlobal,
    ApplicationSpecific,
    RandomOnce,
    PaletteOnce,
}

impl Default for ApplicationColorMode {
    fn default() -> Self {
        Self::ApplicationSpecific
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationColorEntry {
    pub label: String,
    pub executable_name: String,
    pub executable_path: Option<String>,
    pub automatic_color: RgbaColor,
    pub manual_color: Option<RgbaColor>,
}

impl ApplicationColorEntry {
    pub fn resolved_color(&self) -> RgbaColor {
        self.manual_color
            .clone()
            .unwrap_or_else(|| self.automatic_color.clone())
    }
    pub fn is_manual(&self) -> bool {
        self.manual_color.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationColorRegistry {
    pub mode: ApplicationColorMode,
    pub fixed_global_color: RgbaColor,
    pub palette: Vec<RgbaColor>,
    pub entries: BTreeMap<String, ApplicationColorEntry>,
}

impl Default for ApplicationColorRegistry {
    fn default() -> Self {
        Self {
            mode: ApplicationColorMode::ApplicationSpecific,
            fixed_global_color: RgbaColor::new(0, 120, 215, 255),
            palette: vec![
                RgbaColor::new(0, 120, 215, 255),
                RgbaColor::new(255, 185, 0, 255),
                RgbaColor::new(16, 124, 16, 255),
                RgbaColor::new(232, 17, 35, 255),
                RgbaColor::new(104, 33, 122, 255),
                RgbaColor::new(0, 153, 188, 255),
            ],
            entries: BTreeMap::new(),
        }
    }
}

impl ApplicationColorRegistry {
    pub fn color_for(&mut self, app: &ApplicationIdentity, fallback: &RgbaColor) -> RgbaColor {
        if matches!(self.mode, ApplicationColorMode::FixedGlobal) {
            return self.fixed_global_color.clone();
        }
        let key = app.stable_key();
        if !self.entries.contains_key(&key) {
            let automatic_color = match self.mode {
                ApplicationColorMode::FixedGlobal => self.fixed_global_color.clone(),
                ApplicationColorMode::ApplicationSpecific | ApplicationColorMode::PaletteOnce => {
                    self.palette_color(&key, fallback)
                }
                ApplicationColorMode::RandomOnce => random_color_from_key(&key),
            };
            self.entries.insert(
                key.clone(),
                ApplicationColorEntry {
                    label: app.executable_name.clone(),
                    executable_name: app.executable_name.clone(),
                    executable_path: app.executable_path.clone(),
                    automatic_color,
                    manual_color: None,
                },
            );
        }
        self.entries
            .get(&key)
            .map(ApplicationColorEntry::resolved_color)
            .unwrap_or_else(|| fallback.clone())
    }

    pub fn set_manual_override(&mut self, app: &ApplicationIdentity, color: RgbaColor) {
        let _ = self.color_for(app, &color);
        if let Some(entry) = self.entries.get_mut(&app.stable_key()) {
            entry.manual_color = Some(color);
        }
    }

    pub fn reset_to_automatic(&mut self, app: &ApplicationIdentity) {
        if let Some(entry) = self.entries.get_mut(&app.stable_key()) {
            entry.manual_color = None;
        }
    }

    fn palette_color(&self, key: &str, fallback: &RgbaColor) -> RgbaColor {
        if self.palette.is_empty() {
            return fallback.clone();
        }
        let idx = (stable_hash(key) as usize) % self.palette.len();
        self.palette[idx].clone()
    }
}

fn random_color_from_key(key: &str) -> RgbaColor {
    let hash = stable_hash(key);
    RgbaColor::new(
        64 + (hash & 0x7f) as u8,
        64 + ((hash >> 8) & 0x7f) as u8,
        64 + ((hash >> 16) & 0x7f) as u8,
        255,
    )
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for b in value.as_bytes() {
        hash = (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> ApplicationIdentity {
        ApplicationIdentity::new(10, "demo.exe", Some("c:/demo.exe".into()), None)
    }

    #[test]
    fn palette_assignment_is_stable_for_same_application() {
        let mut r = ApplicationColorRegistry {
            mode: ApplicationColorMode::PaletteOnce,
            ..Default::default()
        };
        assert_eq!(
            r.color_for(&app(), &RgbaColor::new(1, 2, 3, 255)),
            r.color_for(&app(), &RgbaColor::new(9, 9, 9, 255))
        );
    }

    #[test]
    fn random_assignment_occurs_once_and_is_reused() {
        let mut r = ApplicationColorRegistry {
            mode: ApplicationColorMode::RandomOnce,
            ..Default::default()
        };
        let first = r.color_for(&app(), &RgbaColor::new(1, 2, 3, 255));
        r.mode = ApplicationColorMode::PaletteOnce;
        assert_eq!(first, r.color_for(&app(), &RgbaColor::new(9, 9, 9, 255)));
    }

    #[test]
    fn manual_override_and_reset_to_automatic_work() {
        let mut r = ApplicationColorRegistry::default();
        let auto = r.color_for(&app(), &RgbaColor::new(1, 2, 3, 255));
        let manual = RgbaColor::new(8, 7, 6, 255);
        r.set_manual_override(&app(), manual.clone());
        assert_eq!(r.color_for(&app(), &auto), manual);
        r.reset_to_automatic(&app());
        assert_eq!(r.color_for(&app(), &auto), auto);
    }
}
