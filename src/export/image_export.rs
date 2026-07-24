pub use crate::export::model::{ExportBackground, ExportFormat, ExportOptions};
use crate::{canvas::model::CanvasModel, export::compositor};
use chrono::{DateTime, Local};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("failed to create export directory {path}: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("failed to encode export {path}: {source}")]
    Write {
        path: PathBuf,
        source: image::ImageError,
    },
}
pub fn export_image(canvas: &CanvasModel, o: &ExportOptions) -> Result<PathBuf, ExportError> {
    let ext = match o.format {
        ExportFormat::Png => "png",
        ExportFormat::WebP => "webp",
    };
    let path = o
        .destination
        .clone()
        .unwrap_or_else(|| collision_safe_path(&o.default_directory, &base_filename(o), ext));
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(|source| ExportError::CreateDirectory {
            path: p.into(),
            source,
        })?
    }
    compositor::compose(canvas, o)
        .save_with_format(
            &path,
            match o.format {
                ExportFormat::Png => image::ImageFormat::Png,
                ExportFormat::WebP => image::ImageFormat::WebP,
            },
        )
        .map_err(|source| ExportError::Write {
            path: path.clone(),
            source,
        })?;
    Ok(path)
}
pub fn export_png(c: &CanvasModel, o: &ExportOptions) -> Result<PathBuf, ExportError> {
    export_image(c, o)
}
pub fn render_canvas_to_image(c: &CanvasModel, o: &ExportOptions) -> image::RgbaImage {
    compositor::compose(c, o)
}
pub fn base_filename(o: &ExportOptions) -> String {
    let dt: DateTime<Local> = o.timestamp.into();
    format!("MultiMouseCanvas_{}", dt.format("%Y-%m-%d_%H-%M-%S"))
}
pub fn sanitize_filename_component(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .into()
}
pub fn collision_safe_path(d: &Path, stem: &str, ext: &str) -> PathBuf {
    for n in 0.. {
        let p = d.join(if n == 0 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        });
        if !p.exists() {
            return p;
        }
    }
    unreachable!()
}
