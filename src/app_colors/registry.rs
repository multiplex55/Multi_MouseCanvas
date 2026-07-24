use crate::{capture::foreground::ApplicationIdentity, settings::model::RgbaColor};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::SystemTime};
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
pub struct ApplicationColorRule {
    pub id: String,
    pub label: String,
    pub match_keys: Vec<String>,
    pub automatic_color: RgbaColor,
    pub manual_color: Option<RgbaColor>,
    #[serde(default = "now")]
    pub created_at: SystemTime,
    #[serde(default = "now")]
    pub last_seen_at: SystemTime,
}
fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH
}
impl ApplicationColorRule {
    pub fn resolved_color(&self) -> RgbaColor {
        self.manual_color
            .clone()
            .unwrap_or_else(|| self.automatic_color.clone())
    }
    pub fn is_manual(&self) -> bool {
        self.manual_color.is_some()
    }
}
pub type ApplicationColorEntry = ApplicationColorRule;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationColorRegistry {
    pub mode: ApplicationColorMode,
    pub fixed_global_color: RgbaColor,
    pub palette: Vec<RgbaColor>,
    #[serde(default)]
    pub rules: BTreeMap<String, ApplicationColorRule>,
    #[serde(default)]
    pub key_owners: BTreeMap<String, String>,
    #[serde(default)]
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
            rules: BTreeMap::new(),
            key_owners: BTreeMap::new(),
            entries: BTreeMap::new(),
        }
    }
}
impl ApplicationColorRegistry {
    fn ensure_migrated(&mut self) {
        if self.rules.is_empty() && !self.entries.is_empty() {
            for (k, e) in self.entries.clone() {
                let id = rule_id(&k);
                let keys = vec![
                    k.clone(),
                    e.match_keys.first().cloned().unwrap_or(k.clone()),
                ];
                self.rules.insert(
                    id.clone(),
                    ApplicationColorRule {
                        id: id.clone(),
                        label: e.label,
                        match_keys: keys.clone(),
                        automatic_color: e.automatic_color,
                        manual_color: e.manual_color,
                        created_at: SystemTime::UNIX_EPOCH,
                        last_seen_at: SystemTime::UNIX_EPOCH,
                    },
                );
                for key in keys {
                    self.key_owners.insert(key, id.clone());
                }
            }
        }
        if self.key_owners.is_empty() {
            for (id, r) in &self.rules {
                for k in &r.match_keys {
                    self.key_owners.insert(k.clone(), id.clone());
                }
            }
        }
    }
    pub fn color_for(&mut self, app: &ApplicationIdentity, fallback: &RgbaColor) -> RgbaColor {
        if matches!(self.mode, ApplicationColorMode::FixedGlobal) {
            return self.fixed_global_color.clone();
        }
        self.ensure_migrated();
        let key = app.stable_key();
        let id = self.key_owners.get(&key).cloned().unwrap_or_else(|| {
            let id = rule_id(&key);
            let automatic_color = match self.mode {
                ApplicationColorMode::FixedGlobal => self.fixed_global_color.clone(),
                ApplicationColorMode::ApplicationSpecific | ApplicationColorMode::PaletteOnce => {
                    self.palette_color(&key, fallback)
                }
                ApplicationColorMode::RandomOnce => random_color_from_key(&key),
            };
            let rule = ApplicationColorRule {
                id: id.clone(),
                label: app.executable_name.clone(),
                match_keys: vec![key.clone()],
                automatic_color,
                manual_color: None,
                created_at: SystemTime::now(),
                last_seen_at: SystemTime::now(),
            };
            self.rules.insert(id.clone(), rule.clone());
            self.entries.insert(key.clone(), rule);
            self.key_owners.insert(key.clone(), id.clone());
            id
        });
        if let Some(r) = self.rules.get_mut(&id) {
            r.last_seen_at = SystemTime::now();
            return r.resolved_color();
        }
        fallback.clone()
    }
    pub fn set_manual_override(&mut self, app: &ApplicationIdentity, color: RgbaColor) {
        let _ = self.color_for(app, &color);
        if let Some(id) = self.key_owners.get(&app.stable_key()).cloned() {
            self.set_manual_override_by_rule_id(&id, color);
        }
    }
    pub fn set_manual_override_by_rule_id(&mut self, id: &str, color: RgbaColor) {
        self.ensure_migrated();
        if let Some(r) = self.rules.get_mut(id) {
            r.manual_color = Some(color.clone());
        }
        for e in self.entries.values_mut().filter(|e| e.id == id) {
            e.manual_color = Some(color.clone());
        }
    }
    pub fn reset_to_automatic(&mut self, app: &ApplicationIdentity) {
        self.ensure_migrated();
        if let Some(id) = self.key_owners.get(&app.stable_key()).cloned() {
            if let Some(r) = self.rules.get_mut(&id) {
                r.manual_color = None;
            }
        }
    }
    pub fn rename_rule(&mut self, id: &str, label: String) -> Result<(), String> {
        self.ensure_migrated();
        let r = self.rules.get_mut(id).ok_or("missing rule")?;
        r.label = label;
        Ok(())
    }
    pub fn merge_rules(&mut self, survivor: &str, merged: &str) -> Result<(), String> {
        self.ensure_migrated();
        if survivor == merged {
            return Ok(());
        }
        let other = self.rules.remove(merged).ok_or("missing merged rule")?;
        let s = self.rules.get_mut(survivor).ok_or("missing survivor")?;
        for key in other.match_keys {
            if self
                .key_owners
                .get(&key)
                .is_some_and(|owner| owner != merged)
            {
                return Err("duplicate match-key ownership".into());
            }
            if !s.match_keys.contains(&key) {
                s.match_keys.push(key.clone());
            }
            self.key_owners.insert(key, survivor.to_owned());
        }
        Ok(())
    }
    fn palette_color(&self, key: &str, fallback: &RgbaColor) -> RgbaColor {
        if self.palette.is_empty() {
            fallback.clone()
        } else {
            self.palette[(stable_hash(key) as usize) % self.palette.len()].clone()
        }
    }
}
fn rule_id(key: &str) -> String {
    format!("app-{:016x}", stable_hash(key))
}
fn random_color_from_key(key: &str) -> RgbaColor {
    let h = stable_hash(key);
    RgbaColor::new(
        64 + (h & 0x7f) as u8,
        64 + ((h >> 8) & 0x7f) as u8,
        64 + ((h >> 16) & 0x7f) as u8,
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
    fn application_rule_color_override() {
        let mut r = ApplicationColorRegistry::default();
        r.color_for(&app(), &RgbaColor::new(1, 2, 3, 255));
        let id = r.key_owners.get(&app().stable_key()).unwrap().clone();
        let m = RgbaColor::new(8, 7, 6, 255);
        r.set_manual_override_by_rule_id(&id, m.clone());
        assert_eq!(r.color_for(&app(), &m), m);
    }
    #[test]
    fn application_rule_label_rename() {
        let mut r = ApplicationColorRegistry::default();
        r.color_for(&app(), &RgbaColor::new(1, 2, 3, 255));
        let id = r.key_owners.get(&app().stable_key()).unwrap().clone();
        r.rename_rule(&id, "Demo".into()).unwrap();
        assert_eq!(r.rules[&id].label, "Demo");
    }
    #[test]
    fn merge_prevents_duplicate_match_key_ownership() {
        let mut r = ApplicationColorRegistry::default();
        let a = ApplicationIdentity::new(1, "a.exe", None, None);
        let b = ApplicationIdentity::new(2, "b.exe", None, None);
        r.color_for(&a, &RgbaColor::new(1, 1, 1, 255));
        r.color_for(&b, &RgbaColor::new(2, 2, 2, 255));
        let ia = r.key_owners[&a.stable_key()].clone();
        let ib = r.key_owners[&b.stable_key()].clone();
        assert!(r.merge_rules(&ia, &ib).is_ok());
        assert_eq!(r.key_owners[&b.stable_key()], ia);
    }
    #[test]
    fn merged_application_rule_affects_future_resolution() {
        let mut r = ApplicationColorRegistry::default();
        let a = ApplicationIdentity::new(1, "a.exe", None, None);
        let b = ApplicationIdentity::new(2, "b.exe", None, None);
        let ca = r.color_for(&a, &RgbaColor::new(1, 1, 1, 255));
        r.color_for(&b, &RgbaColor::new(2, 2, 2, 255));
        let ia = r.key_owners[&a.stable_key()].clone();
        let ib = r.key_owners[&b.stable_key()].clone();
        r.merge_rules(&ia, &ib).unwrap();
        assert_eq!(r.color_for(&b, &RgbaColor::new(9, 9, 9, 255)), ca);
    }
}
