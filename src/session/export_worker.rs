use crate::{
    export::{
        compositor,
        image_export::base_filename,
        model::{ExportDestination, ExportFormat, ExportOptions},
    },
    session::{
        events::{ExportRequest, ExportResult},
        export_snapshot::ExportSnapshot,
    },
};
use std::{
    fs::{self, OpenOptions},
    io::{self, BufWriter},
    path::{Path, PathBuf},
    sync::mpsc::{channel, Receiver},
    thread,
};

pub fn spawn(snapshot: ExportSnapshot, request: ExportRequest) -> Receiver<ExportResult> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let id = request.id;
        let retry = request.clone();
        let result = std::panic::catch_unwind(|| write(snapshot, &request));
        let message = match result {
            Ok(Ok(path)) => ExportResult::Success {
                request_id: id,
                path,
            },
            Ok(Err(error)) => ExportResult::Failure {
                request_id: id,
                error: error.to_string(),
                retry_request: retry,
            },
            Err(_) => ExportResult::Failure {
                request_id: id,
                error: "export worker panicked".into(),
                retry_request: retry,
            },
        };
        let _ = tx.send(message);
    });
    rx
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("only PNG export is supported")]
    UnsupportedFormat,
    #[error("export path has no parent")]
    MissingParent,
    #[error("export filesystem error at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("PNG encoding failed at {path}: {source}")]
    Encode {
        path: PathBuf,
        source: image::ImageError,
    },
}

fn write(snapshot: ExportSnapshot, request: &ExportRequest) -> Result<PathBuf, WorkerError> {
    if request.format != ExportFormat::Png {
        return Err(WorkerError::UnsupportedFormat);
    }
    let (dir, exact) = match &request.destination {
        ExportDestination::Directory(d) => (d.clone(), None),
        ExportDestination::ExactFile(p) => (
            p.parent().ok_or(WorkerError::MissingParent)?.to_owned(),
            Some(p.clone()),
        ),
    };
    fs::create_dir_all(&dir).map_err(|source| WorkerError::Io {
        path: dir.clone(),
        source,
    })?;
    let options = ExportOptions {
        destination: None,
        default_directory: dir.clone(),
        timestamp: request.timestamp,
        format: request.format,
        scale: request.scale,
        background: request.background.clone(),
        panels: request.panels.clone(),
    };
    let stem = base_filename(&options);
    let (path, file) = reserve(&dir, exact.as_deref(), &stem)?;
    let image = compositor::compose_snapshot(&snapshot, &options);
    image
        .write_to(&mut BufWriter::new(file), image::ImageFormat::Png)
        .map_err(|source| WorkerError::Encode {
            path: path.clone(),
            source,
        })?;
    Ok(path)
}

fn reserve(
    dir: &Path,
    exact: Option<&Path>,
    stem: &str,
) -> Result<(PathBuf, fs::File), WorkerError> {
    for n in 0u64.. {
        let path = exact.map(Path::to_owned).unwrap_or_else(|| {
            dir.join(if n == 0 {
                format!("{stem}.png")
            } else {
                format!("{stem}-{n}.png")
            })
        });
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && exact.is_none() => continue,
            Err(source) => return Err(WorkerError::Io { path, source }),
        }
    }
    unreachable!()
}
