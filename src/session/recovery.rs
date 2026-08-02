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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitStage {
    BeforeTileCommit,
    BeforeManifestCommit,
}

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
    save_session_with_failpoint(dir, manifest, store, |_| Ok(()))
}

/// Test seam for deterministic I/O failure injection. Each stage is attempted
/// once; checkpoint creation never retries indefinitely.
pub fn save_session_with_failpoint(
    dir: &Path,
    manifest: &SessionManifest,
    store: &mut SparseTileStore,
    mut failpoint: impl FnMut(CommitStage) -> io::Result<()>,
) -> io::Result<()> {
    fs::create_dir_all(dir.join("tiles"))?;
    atomic_bytes(
        &dir.join(VERSION_FILENAME),
        RECOVERY_SCHEMA_VERSION.to_string().as_bytes(),
        |_| Ok(()),
    )?;
    let replacing = dir.join(MANIFEST_FILENAME).exists();
    let transaction = format!(
        "checkpoint-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let tiles: Vec<_> = store
        .tiles
        .iter()
        .map(|(c, t)| (*c, t.revision, t.pixels.clone()))
        .collect();
    let mut committed_names = Vec::new();
    for (coord, revision, pixels) in tiles {
        let image =
            RgbaImage::from_raw(store.tile_size, store.tile_size, pixels).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid RGBA tile length")
            })?;
        let name = if replacing {
            format!("{transaction}-{}", tile_filename(coord))
        } else {
            tile_filename(coord)
        };
        let path = dir.join("tiles").join(&name);
        failpoint(CommitStage::BeforeTileCommit)?;
        atomic_png(&path, &image)?;
        committed_names.push(name);
        if let Some(tile) = store.tiles.get_mut(&coord) {
            if tile.revision == revision {
                tile.recovery_dirty = false;
            }
        }
    }
    let mut committed_manifest = manifest.clone();
    committed_manifest.tiles = committed_names;
    failpoint(CommitStage::BeforeManifestCommit)?;
    // The manifest is the commit record and is always replaced last. Until
    // this rename succeeds the previous manifest and every tile it references
    // remain untouched and loadable.
    atomic_json(&dir.join(MANIFEST_FILENAME), &committed_manifest)
}

pub fn load_session(dir: &Path) -> io::Result<(SessionManifest, SparseTileStore)> {
    let bytes = fs::read(dir.join(MANIFEST_FILENAME))?;
    let mut manifest: SessionManifest = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if manifest.schema_version != RECOVERY_SCHEMA_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported recovery schema {}", manifest.schema_version),
        ));
    }
    // `completed` predates `Finished` and is reliable persisted evidence that
    // the stopped sampler belongs to a finalized, retained session.
    if manifest.completed && manifest.recording_status == RecordingStatus::Stopped {
        manifest.recording_status = RecordingStatus::Finished;
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
    canvas.session_desktop_bounds = legacy.virtual_desktop_bounds;
    let manifest = SessionManifest::checkpoint(
        session_id,
        SystemTime::UNIX_EPOCH,
        legacy.saved_at,
        legacy.completed,
        if legacy.completed {
            RecordingStatus::Finished
        } else {
            RecordingStatus::Stopped
        },
        &canvas,
        legacy.statistics,
        legacy.application_colors,
        None,
    );
    save_session(session_dir, &manifest, &mut canvas.sparse_tiles)?;
    // Validate the conversion before returning. The original is intentionally retained.
    load_session(session_dir).map(|_| ())
}

fn parse_tile_filename(name: &str) -> io::Result<TileCoordinate> {
    let stem = name
        .strip_suffix(".png")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad tile name"))?;
    let stem = stem.rsplit('-').next().unwrap_or(stem);
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
            profile_snapshot: None,
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

    #[test]
    fn failed_replacement_preserves_previous_manifest_and_tiles() {
        let d = tempdir().unwrap();
        let mut canvas = CanvasModel::default();
        canvas
            .sparse_tiles
            .put_pixel(1, 1, [9, 8, 7, 255], |a, b| a.copy_from_slice(&b));
        let manifest = SessionManifest::checkpoint(
            "one".into(),
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
            false,
            RecordingStatus::Stopped,
            &canvas,
            SessionStatistics::default(),
            ApplicationColorRegistry::default(),
            None,
        );
        save_session(d.path(), &manifest, &mut canvas.sparse_tiles).unwrap();
        let old_manifest = fs::read(d.path().join(MANIFEST_FILENAME)).unwrap();
        let old_loaded = load_session(d.path()).unwrap();
        canvas
            .sparse_tiles
            .put_pixel(2, 2, [1, 2, 3, 255], |a, b| a.copy_from_slice(&b));
        let error =
            save_session_with_failpoint(d.path(), &manifest, &mut canvas.sparse_tiles, |stage| {
                if stage == CommitStage::BeforeManifestCommit {
                    Err(io::Error::other("injected manifest failure"))
                } else {
                    Ok(())
                }
            });
        assert!(error.is_err());
        assert_eq!(
            fs::read(d.path().join(MANIFEST_FILENAME)).unwrap(),
            old_manifest
        );
        let loaded = load_session(d.path()).unwrap();
        assert_eq!(loaded.0.session_id, old_loaded.0.session_id);
        assert_eq!(loaded.1.tiles.len(), old_loaded.1.tiles.len());
    }
}
