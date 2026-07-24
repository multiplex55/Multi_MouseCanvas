use crate::{
    app_colors::registry::ApplicationColorRegistry,
    canvas::{
        coordinates::{TileCoordinate, VirtualDesktopBounds},
        model::CanvasModel,
        tiles::{SparseTileStore, Tile},
    },
    session::{
        manifest::{SessionManifest, RECOVERY_SCHEMA_VERSION},
        model::RecordingStatus,
        statistics::SessionStatistics,
    },
};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const LEGACY_FILENAME: &str = "autosave.recovery.json";
pub const MANIFEST_FILENAME: &str = "manifest.json";
pub const VERSION_FILENAME: &str = "recovery-version";

/// The old representation is retained solely as an explicit import format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyRecoveryState {
    pub canvas: CanvasModel,
    pub session_name: Option<String>,
    pub saved_at: SystemTime,
    pub application_colors: ApplicationColorRegistry,
    pub statistics: SessionStatistics,
    pub virtual_desktop_bounds: VirtualDesktopBounds,
    pub completed: bool,
}
pub type RecoveryState = LegacyRecoveryState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStatus {
    None,
    Incomplete(PathBuf),
    Completed(PathBuf),
    Malformed(PathBuf, String),
    Legacy(PathBuf),
}

pub fn autosave_path(base: &Path) -> PathBuf {
    base.join(LEGACY_FILENAME)
}
pub fn legacy_status(root: &Path) -> RecoveryStatus {
    let path = autosave_path(root);
    if !path.exists() {
        return RecoveryStatus::None;
    }
    match load_legacy(&path) {
        Ok(_) => RecoveryStatus::Legacy(path),
        Err(e) => RecoveryStatus::Malformed(
            path,
            format!("Legacy recovery is malformed or unsupported: {e}"),
        ),
    }
}
pub fn load_legacy(path: &Path) -> io::Result<LegacyRecoveryState> {
    serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)
}
/// Compatibility writer used only by legacy-focused tests/tools.
pub fn save_recovery(path: &Path, state: &LegacyRecoveryState) -> io::Result<()> {
    atomic_json(path, state)
}
pub fn load_recovery(path: &Path) -> io::Result<LegacyRecoveryState> {
    load_legacy(path)
}
pub fn detect_incomplete(path: &Path) -> RecoveryStatus {
    match load_legacy(path) {
        Ok(s) if !s.completed => RecoveryStatus::Incomplete(path.into()),
        _ => RecoveryStatus::None,
    }
}

pub fn tile_filename(c: TileCoordinate) -> String {
    format!("{}_{}.png", c.x, c.y)
}
pub fn snapshot_dirty_tiles(store: &SparseTileStore) -> Vec<(TileCoordinate, u64, Vec<u8>)> {
    store
        .tiles
        .iter()
        .filter(|(_, t)| t.recovery_dirty)
        .map(|(c, t)| (*c, t.revision, t.pixels.clone()))
        .collect()
}

/// Commits dirty PNGs first and the manifest last. A dirty bit is cleared only
/// when the exact snapshotted revision reached durable storage.
pub fn save_session(
    dir: &Path,
    manifest: &SessionManifest,
    store: &mut SparseTileStore,
) -> io::Result<()> {
    fs::create_dir_all(dir.join("tiles"))?;
    atomic_bytes(
        &dir.join(VERSION_FILENAME),
        RECOVERY_SCHEMA_VERSION.to_string().as_bytes(),
        |_| Ok(()),
    )?;
    let dirty = snapshot_dirty_tiles(store);
    for (coord, revision, pixels) in dirty {
        let image =
            RgbaImage::from_raw(store.tile_size, store.tile_size, pixels).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid RGBA tile length")
            })?;
        let path = dir.join("tiles").join(tile_filename(coord));
        atomic_png(&path, &image)?;
        if let Some(tile) = store.tiles.get_mut(&coord) {
            if tile.revision == revision {
                tile.recovery_dirty = false;
            }
        }
    }
    atomic_json(&dir.join(MANIFEST_FILENAME), manifest)
}

pub fn load_session(dir: &Path) -> io::Result<(SessionManifest, SparseTileStore)> {
    let bytes = fs::read(dir.join(MANIFEST_FILENAME))?;
    let manifest: SessionManifest = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if manifest.schema_version != RECOVERY_SCHEMA_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported recovery schema {}", manifest.schema_version),
        ));
    }
    let mut store = SparseTileStore {
        tile_size: manifest.tile_size,
        tiles: Default::default(),
    };
    for name in &manifest.tiles {
        let coord = parse_tile_filename(name)?;
        let decoded = image::open(dir.join("tiles").join(name))
            .map_err(io::Error::other)?
            .to_rgba8();
        if decoded.dimensions() != (manifest.tile_size, manifest.tile_size) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "tile dimensions mismatch",
            ));
        }
        store.tiles.insert(
            coord,
            Tile {
                pixels: decoded.into_raw(),
                preview_dirty: true,
                recovery_dirty: false,
                revision: 0,
                contains_artwork: true,
            },
        );
    }
    Ok((manifest, store))
}

pub fn restore_canvas(dir: &Path) -> io::Result<(SessionManifest, CanvasModel)> {
    let (m, tiles) = load_session(dir)?;
    let mut canvas = CanvasModel::default();
    canvas.sparse_tiles = tiles;
    canvas.session_desktop_bounds = m.session_bounds;
    canvas.current_topology = m.current_topology.clone();
    canvas.topology_history = m.topology_history.clone();
    canvas.background = m.background.clone();
    // Active overlays are deliberately never represented in a manifest.
    canvas.active_movement_overlay = None;
    canvas.active_dwell_overlay = None;
    canvas.refresh_dimensions();
    Ok((m, canvas))
}

pub fn import_legacy(legacy_path: &Path, session_dir: &Path, session_id: String) -> io::Result<()> {
    let legacy = load_legacy(legacy_path)?;
    let mut canvas = legacy.canvas.clone();
    for tile in canvas.sparse_tiles.tiles.values_mut() {
        tile.recovery_dirty = true;
    }
    let manifest = SessionManifest {
        schema_version: RECOVERY_SCHEMA_VERSION,
        session_id,
        started_at: SystemTime::UNIX_EPOCH,
        saved_at: legacy.saved_at,
        completed: legacy.completed,
        recording_status: RecordingStatus::Stopped,
        session_bounds: legacy.virtual_desktop_bounds,
        current_topology: canvas.current_topology.clone(),
        topology_history: canvas.topology_history.clone(),
        statistics: legacy.statistics,
        background: canvas.background.clone(),
        tile_size: canvas.sparse_tiles.tile_size,
        pixel_format: "RGBA8".into(),
        application_colors: legacy.application_colors,
        tiles: canvas
            .sparse_tiles
            .tiles
            .keys()
            .map(|c| tile_filename(*c))
            .collect(),
    };
    save_session(session_dir, &manifest, &mut canvas.sparse_tiles)?;
    // Validate the conversion before returning. The original is intentionally retained.
    load_session(session_dir).map(|_| ())
}

fn parse_tile_filename(name: &str) -> io::Result<TileCoordinate> {
    let stem = name
        .strip_suffix(".png")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad tile name"))?;
    let (x, y) = stem
        .split_once('_')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad tile name"))?;
    Ok(TileCoordinate {
        x: x.parse().map_err(io::Error::other)?,
        y: y.parse().map_err(io::Error::other)?,
    })
}
fn atomic_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    atomic_bytes(path, &bytes, |b| {
        serde_json::from_slice::<serde_json::Value>(b)
            .map(|_| ())
            .map_err(io::Error::other)
    })
}
fn atomic_png(path: &Path, image: &RgbaImage) -> io::Result<()> {
    let tmp = temp_path(path);
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    {
        let mut file = fs::File::create(&tmp)?;
        image::DynamicImage::ImageRgba8(image.clone())
            .write_to(&mut file, image::ImageFormat::Png)
            .map_err(io::Error::other)?;
        file.sync_all()?;
    }
    let _ = image::load_from_memory_with_format(&fs::read(&tmp)?, image::ImageFormat::Png)
        .map_err(io::Error::other)?
        .to_rgba8();
    replace(&tmp, path)
}
fn atomic_bytes(
    path: &Path,
    bytes: &[u8],
    validate: impl FnOnce(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let tmp = temp_path(path);
    {
        use io::Write;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    let check = fs::read(&tmp)?;
    validate(&check)?;
    replace(&tmp, path)
}
fn temp_path(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}.tmp-{}",
        path.extension().and_then(|x| x.to_str()).unwrap_or(""),
        std::process::id()
    ))
}
fn replace(tmp: &Path, path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(tmp, path)
}
pub fn discard_recovery(path: &Path) -> io::Result<()> {
    let r = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match r {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn dirty_snapshot_only_contains_dirty() {
        let mut s = SparseTileStore::default();
        s.put_pixel(1, 1, [1, 2, 3, 4], |d, c| d.copy_from_slice(&c));
        assert_eq!(snapshot_dirty_tiles(&s).len(), 1);
    }
    #[test]
    fn malformed_manifest_is_not_deleted() {
        let d = tempdir().unwrap();
        fs::write(d.path().join(MANIFEST_FILENAME), "{").unwrap();
        assert!(load_session(d.path()).is_err());
        assert!(d.path().join(MANIFEST_FILENAME).exists());
    }
    #[test]
    fn interrupted_atomic_write_keeps_old() {
        let d = tempdir().unwrap();
        let p = d.path().join("x.json");
        fs::write(&p, "{}").unwrap();
        assert!(
            atomic_bytes(&p, b"{", |b| serde_json::from_slice::<serde_json::Value>(b)
                .map(|_| ())
                .map_err(io::Error::other))
            .is_err()
        );
        assert_eq!(fs::read_to_string(p).unwrap(), "{}");
    }
    #[test]
    fn legacy_detection_keeps_original() {
        let d = tempdir().unwrap();
        fs::write(autosave_path(d.path()), "bad").unwrap();
        assert!(matches!(
            legacy_status(d.path()),
            RecoveryStatus::Malformed(..)
        ));
        assert!(autosave_path(d.path()).exists());
    }

    #[test]
    fn unsupported_manifest_and_stale_temporary_file_are_preserved() {
        let d = tempdir().unwrap();
        fs::write(
            d.path().join(MANIFEST_FILENAME),
            r#"{"schema_version":999}"#,
        )
        .unwrap();
        let stale = d.path().join("manifest.json.tmp-interrupted");
        fs::write(&stale, "partial").unwrap();
        assert!(load_session(d.path()).is_err());
        assert!(d.path().join(MANIFEST_FILENAME).exists());
        assert!(stale.exists());
    }

    #[test]
    fn missing_and_malformed_tiles_do_not_delete_recovery() {
        let d = tempdir().unwrap();
        let mut canvas = CanvasModel::default();
        canvas
            .sparse_tiles
            .put_pixel(1, 1, [1, 2, 3, 255], |dst, src| dst.copy_from_slice(&src));
        let manifest = SessionManifest {
            schema_version: RECOVERY_SCHEMA_VERSION,
            session_id: "test".into(),
            started_at: SystemTime::UNIX_EPOCH,
            saved_at: SystemTime::UNIX_EPOCH,
            completed: false,
            recording_status: RecordingStatus::Stopped,
            session_bounds: canvas.session_desktop_bounds,
            current_topology: canvas.current_topology.clone(),
            topology_history: canvas.topology_history.clone(),
            statistics: SessionStatistics::default(),
            background: canvas.background.clone(),
            tile_size: canvas.sparse_tiles.tile_size,
            pixel_format: "RGBA8".into(),
            application_colors: ApplicationColorRegistry::default(),
            tiles: vec![tile_filename(TileCoordinate { x: 0, y: 0 })],
        };
        save_session(d.path(), &manifest, &mut canvas.sparse_tiles).unwrap();
        let tile = d.path().join("tiles").join(&manifest.tiles[0]);
        fs::remove_file(&tile).unwrap();
        assert!(load_session(d.path()).is_err());
        assert!(d.path().join(MANIFEST_FILENAME).exists());
        fs::write(&tile, "not a png").unwrap();
        assert!(load_session(d.path()).is_err());
        assert!(tile.exists());
    }
}
