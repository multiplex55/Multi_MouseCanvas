use crate::{
    canvas::topology::{DisplayLayoutFingerprint, DisplayTopology},
    platform::display_identity::{DisplayIdentity, DisplayOrientation},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const DISPLAY_PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorDescriptor {
    pub stable_key: String,
    pub identity: DisplayIdentity,
    pub physical_rect: crate::canvas::coordinates::DesktopRect,
    pub resolution: (u32, u32),
    pub orientation: DisplayOrientation,
    pub primary: bool,
    pub display_name: String,
}
impl From<&crate::canvas::topology::Monitor> for MonitorDescriptor {
    fn from(m: &crate::canvas::topology::Monitor) -> Self {
        Self {
            stable_key: m.stable_key().to_owned(),
            identity: m.identity.clone(),
            physical_rect: m.physical_rect,
            resolution: (m.width.max(0.0) as u32, m.height.max(0.0) as u32),
            orientation: m.orientation,
            primary: m.primary,
            display_name: m.label.clone().unwrap_or_else(|| m.id.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedDisplayProfile {
    pub id: String,
    pub schema_version: u32,
    pub name: String,
    pub renamed: bool,
    pub fingerprint: DisplayLayoutFingerprint,
    pub detected_monitors: Vec<MonitorDescriptor>,
    pub included_stable_keys: Vec<String>,
    pub preferred: bool,
}
impl SavedDisplayProfile {
    pub fn from_topology(
        id: impl Into<String>,
        topology: &DisplayTopology,
        included_stable_keys: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            schema_version: DISPLAY_PROFILE_SCHEMA_VERSION,
            name: generated_name(topology, &included_stable_keys),
            renamed: false,
            fingerprint: topology.fingerprint.clone(),
            detected_monitors: topology.monitors.iter().map(Into::into).collect(),
            included_stable_keys,
            preferred: false,
        }
    }
    pub fn validates(&self, topology: &DisplayTopology) -> bool {
        self.fingerprint == topology.fingerprint
            && !self.included_stable_keys.is_empty()
            && self
                .included_stable_keys
                .iter()
                .all(|k| topology.monitors.iter().any(|m| m.stable_key() == k))
    }
    pub fn snapshot(&self, detected: &DisplayTopology) -> Option<DisplayProfileSnapshot> {
        self.validates(detected).then(|| DisplayProfileSnapshot {
            profile_id: Some(self.id.clone()),
            profile_name: self.name.clone(),
            fingerprint: detected.fingerprint.clone(),
            detected_topology: detected.clone(),
            included_stable_keys: self.included_stable_keys.clone(),
            effective_topology: detected.effective(&self.included_stable_keys),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayProfileSnapshot {
    pub profile_id: Option<String>,
    pub profile_name: String,
    pub fingerprint: DisplayLayoutFingerprint,
    pub detected_topology: DisplayTopology,
    pub included_stable_keys: Vec<String>,
    pub effective_topology: DisplayTopology,
}
pub type ImmutableDisplayProfileSnapshot = Arc<DisplayProfileSnapshot>;

pub fn generated_name(topology: &DisplayTopology, included: &[String]) -> String {
    let selected: Vec<_> = topology
        .monitors
        .iter()
        .filter(|m| included.iter().any(|k| k == m.stable_key()))
        .collect();
    let bounds = topology.effective(included).bounds();
    let (w, h) = bounds.map_or((0, 0), |b| (b.width() as u32, b.height() as u32));
    format!(
        "{} {} — {} × {}",
        selected.len(),
        if selected.len() == 1 {
            "display"
        } else {
            "displays"
        },
        w,
        h
    )
}
