use super::model::SavedDisplayProfile;
use crate::canvas::topology::DisplayTopology;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayProfileStore {
    pub profiles: Vec<SavedDisplayProfile>,
}

impl DisplayProfileStore {
    pub fn load(path: &Path) -> io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&fs::read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p)?
        }
        let tmp = temporary_path(path);
        let data = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fs::write(&tmp, data)?;
        fs::File::open(&tmp)?.sync_all()?;
        fs::rename(tmp, path)
    }
    pub fn exact_match(&self, t: &DisplayTopology) -> Option<&SavedDisplayProfile> {
        self.profiles
            .iter()
            .filter(|p| p.validates(t))
            .min_by_key(|p| !p.preferred)
    }
    pub fn upsert(&mut self, p: SavedDisplayProfile) {
        if let Some(old) = self.profiles.iter_mut().find(|v| v.id == p.id) {
            *old = p
        } else {
            self.profiles.push(p)
        }
    }
    pub fn rename(&mut self, id: &str, name: String) -> bool {
        self.profiles
            .iter_mut()
            .find(|p| p.id == id)
            .map(|p| {
                p.name = name;
                p.renamed = true
            })
            .is_some()
    }
    pub fn edit_inclusion(&mut self, id: &str, keys: Vec<String>) -> bool {
        if keys.is_empty() {
            return false;
        }
        self.profiles
            .iter_mut()
            .find(|p| p.id == id)
            .map(|p| p.included_stable_keys = keys)
            .is_some()
    }
    pub fn delete(&mut self, id: &str) -> bool {
        let n = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        n != self.profiles.len()
    }
    pub fn set_preferred(&mut self, id: &str) -> bool {
        let fp = match self.profiles.iter().find(|p| p.id == id) {
            Some(p) => p.fingerprint.clone(),
            None => return false,
        };
        for p in &mut self.profiles {
            if p.fingerprint == fp {
                p.preferred = p.id == id
            }
        }
        true
    }
    pub fn forget_layout(&mut self, t: &DisplayTopology) -> usize {
        let n = self.profiles.len();
        self.profiles.retain(|p| p.fingerprint != t.fingerprint);
        n - self.profiles.len()
    }
}
pub fn default_path() -> io::Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "MultiMouseCanvas", "MultiMouseCanvas")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "profile data directory unavailable",
            )
        })?;
    Ok(dirs.config_dir().join("display-profiles.json"))
}
fn temporary_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".tmp");
    PathBuf::from(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::{
            coordinates::DesktopRect,
            topology::{DisplayTopology, Monitor},
        },
        display_profiles::SavedDisplayProfile,
    };
    use tempfile::tempdir;
    fn topology() -> DisplayTopology {
        DisplayTopology::new(vec![
            Monitor::new("a", DesktopRect::new(0., 0., 1920., 1080.), true),
            Monitor::new("b", DesktopRect::new(1920., 0., 3840., 1080.), false),
        ])
    }
    #[test]
    fn missing_is_empty_and_malformed_is_preserved() {
        let d = tempdir().unwrap();
        let p = d.path().join("profiles.json");
        assert!(DisplayProfileStore::load(&p).unwrap().profiles.is_empty());
        std::fs::write(&p, "{").unwrap();
        assert!(DisplayProfileStore::load(&p).is_err());
        assert_eq!(std::fs::read_to_string(p).unwrap(), "{");
    }
    #[test]
    fn exact_preferred_and_crud() {
        let t = topology();
        let keys = t.monitors.iter().map(|m| m.stable_key().into()).collect();
        let a = SavedDisplayProfile::from_topology("a", &t, keys);
        let mut b = a.clone();
        b.id = "b".into();
        b.preferred = true;
        let mut s = DisplayProfileStore {
            profiles: vec![a.clone(), b],
        };
        assert_eq!(s.exact_match(&t).unwrap().id, "b");
        assert!(s.rename("a", "Work".into()));
        assert!(s.set_preferred("a"));
        assert_eq!(s.exact_match(&t).unwrap().id, "a");
        assert!(s.delete("a"));
        assert_eq!(s.forget_layout(&t), 1);
    }
    #[test]
    fn generated_name_and_atomic_save() {
        let d = tempdir().unwrap();
        let p = d.path().join("profiles.json");
        let t = topology();
        let keys = t
            .monitors
            .iter()
            .map(|m| m.stable_key().into())
            .collect::<Vec<_>>();
        let profile = SavedDisplayProfile::from_topology("a", &t, keys);
        assert_eq!(profile.name, "2 displays — 3840 × 1080");
        let s = DisplayProfileStore {
            profiles: vec![profile],
        };
        s.save(&p).unwrap();
        assert_eq!(DisplayProfileStore::load(&p).unwrap(), s);
        assert!(!p.with_extension("json.tmp").exists());
    }
}
