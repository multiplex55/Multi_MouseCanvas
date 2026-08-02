#[cfg(target_os = "windows")]
use std::process::Command;
use std::{io, path::Path};
#[derive(Debug, thiserror::Error)]
pub enum OpenPathError {
    #[error("export path has no containing directory")]
    MissingParent,
    #[error("opening export location is unsupported on this platform")]
    Unsupported,
    #[error("failed to open export location: {0}")]
    Io(#[from] io::Error),
}
pub fn open_export_location(path: &Path) -> Result<(), OpenPathError> {
    let parent = path.parent().ok_or(OpenPathError::MissingParent)?;
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .status()?;
        if status.success() {
            return Ok(());
        }
        Command::new("explorer.exe")
            .arg(parent)
            .status()?
            .success()
            .then_some(())
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Explorer failed").into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = parent;
        Err(OpenPathError::Unsupported)
    }
}
