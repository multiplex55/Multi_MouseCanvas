use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DisplayOrientation {
    #[default]
    Landscape,
    Portrait,
    LandscapeFlipped,
    PortraitFlipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IdentityQuality {
    DevicePathOrEdid,
    AdapterTarget,
    PersistentTarget,
    #[default]
    GdiFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AdapterLuid {
    pub high: i32,
    pub low: u32,
}

/// Persistent identity collected independently of desktop position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DisplayIdentity {
    pub stable_key: String,
    pub adapter_luid: Option<AdapterLuid>,
    pub source_id: Option<u32>,
    pub target_id: Option<u32>,
    pub monitor_device_path: Option<String>,
    pub edid_manufacturer_id: Option<u16>,
    pub edid_product_code: Option<u16>,
    pub target_friendly_name: Option<String>,
    pub gdi_source_name: String,
    pub quality: IdentityQuality,
}

impl DisplayIdentity {
    pub fn gdi_fallback(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            stable_key: format!("gdi:{name}"),
            gdi_source_name: name,
            ..Self::default()
        }
    }
    pub fn rebuild_stable_key(&mut self) {
        self.stable_key = if let Some(path) =
            self.monitor_device_path.as_ref().filter(|v| !v.is_empty())
        {
            self.quality = IdentityQuality::DevicePathOrEdid;
            format!("path:{}", path.to_ascii_lowercase())
        } else if let (Some(m), Some(p), Some(target)) = (
            self.edid_manufacturer_id,
            self.edid_product_code,
            self.target_id,
        ) {
            self.quality = IdentityQuality::DevicePathOrEdid;
            format!("edid:{m:04x}:{p:04x}:{target}")
        } else if let (Some(luid), Some(target)) = (&self.adapter_luid, self.target_id) {
            self.quality = IdentityQuality::AdapterTarget;
            format!(
                "adapter:{:08x}{:08x}:target:{target}",
                luid.high as u32, luid.low
            )
        } else if let Some(name) = self.target_friendly_name.as_ref().filter(|v| !v.is_empty()) {
            self.quality = IdentityQuality::PersistentTarget;
            format!("target:{}", name.to_ascii_lowercase())
        } else {
            self.quality = IdentityQuality::GdiFallback;
            format!("gdi:{}", self.gdi_source_name)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_path_survives_gdi_rename() {
        let mut a = DisplayIdentity::gdi_fallback("DISPLAY1");
        a.monitor_device_path = Some("MONITOR#ABC".into());
        a.rebuild_stable_key();
        let mut b = a.clone();
        b.gdi_source_name = "DISPLAY9".into();
        b.rebuild_stable_key();
        assert_eq!(a.stable_key, b.stable_key);
    }
}
