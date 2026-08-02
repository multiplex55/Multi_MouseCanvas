use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tracing_subscriber::{fmt::MakeWriter, EnvFilter};

pub const LOG_FILE_NAME: &str = "MultiMouseCanvas.log";
const DEFAULT_LIMIT: u64 = 10 * 1024 * 1024;

#[derive(Debug)]
pub struct LoggingStatus {
    pub writer_initialized: bool,
    pub subscriber_installed: bool,
    pub expected_path: PathBuf,
    pub warning: Option<String>,
}

pub fn log_path_for_executable(executable: &Path) -> PathBuf {
    executable
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("logs")
        .join(LOG_FILE_NAME)
}

pub fn expected_log_path() -> PathBuf {
    std::env::current_exe()
        .map(|p| log_path_for_executable(&p))
        .unwrap_or_else(|_| PathBuf::from("logs").join(LOG_FILE_NAME))
}

pub fn filter_from(value: Option<&str>, debug: bool) -> Result<EnvFilter, String> {
    if let Some(value) = value {
        return EnvFilter::try_new(value).map_err(|e| e.to_string());
    }
    EnvFilter::try_new(if debug { "debug" } else { "info" }).map_err(|e| e.to_string())
}

pub fn initialize() -> LoggingStatus {
    let expected_path = expected_log_path();
    let filter = match filter_from(
        std::env::var("RUST_LOG").ok().as_deref(),
        cfg!(debug_assertions),
    ) {
        Ok(filter) => filter,
        Err(error) => {
            return failed(expected_path, format!("invalid RUST_LOG filter: {error}"));
        }
    };
    let writer = match RotatingWriter::new(expected_path.clone(), DEFAULT_LIMIT) {
        Ok(writer) => writer,
        Err(error) => return failed(expected_path, format!("logging unavailable: {error}")),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .try_init();
    match subscriber {
        Ok(()) => LoggingStatus {
            writer_initialized: true,
            subscriber_installed: true,
            expected_path,
            warning: None,
        },
        Err(error) => failed(
            expected_path,
            format!("logging subscriber unavailable: {error}"),
        ),
    }
}

fn failed(expected_path: PathBuf, warning: String) -> LoggingStatus {
    crate::platform::message_box::debug_output(&warning);
    LoggingStatus {
        writer_initialized: false,
        subscriber_installed: false,
        expected_path,
        warning: Some(warning),
    }
}

struct State {
    file: File,
    size: u64,
}

#[derive(Clone)]
pub struct RotatingWriter {
    path: PathBuf,
    limit: u64,
    state: Arc<Mutex<State>>,
}

impl RotatingWriter {
    pub fn new(path: PathBuf, limit: u64) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            path,
            limit,
            state: Arc::new(Mutex::new(State { file, size })),
        })
    }

    fn commit(&self, bytes: &[u8]) -> io::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.size > 0 && state.size.saturating_add(bytes.len() as u64) > self.limit {
            self.rotate(&mut state)?;
        }
        state.file.write_all(bytes)?;
        state.size = state.size.saturating_add(bytes.len() as u64);
        Ok(())
    }

    fn rotate(&self, state: &mut State) -> io::Result<()> {
        state.file.flush()?;
        let temp = self.path.with_extension("log.new");
        let new_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp)?;
        drop(new_file);
        let oldest = generation(&self.path, 4);
        if oldest.exists() {
            fs::remove_file(&oldest)?;
        }
        for n in (1..4).rev() {
            let from = generation(&self.path, n);
            if from.exists() {
                fs::rename(&from, generation(&self.path, n + 1))?;
            }
        }
        fs::rename(&self.path, generation(&self.path, 1))?;
        if let Err(error) = fs::rename(&temp, &self.path) {
            let _ = fs::rename(generation(&self.path, 1), &self.path);
            return Err(error);
        }
        *state = State {
            file: OpenOptions::new().append(true).open(&self.path)?,
            size: 0,
        };
        Ok(())
    }
}

fn generation(path: &Path, n: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), n))
}

pub struct EventWriter {
    owner: RotatingWriter,
    bytes: Vec<u8>,
}
impl Write for EventWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.bytes.is_empty() {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.bytes);
        self.owner.commit(&bytes)
    }
}
impl Drop for EventWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
impl<'a> MakeWriter<'a> for RotatingWriter {
    type Writer = EventWriter;
    fn make_writer(&'a self) -> Self::Writer {
        EventWriter {
            owner: self.clone(),
            bytes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_is_executable_relative() {
        assert_eq!(
            log_path_for_executable(Path::new("/opt/mmc/app")),
            Path::new("/opt/mmc/logs/MultiMouseCanvas.log")
        );
    }
    #[test]
    fn filters_and_override() {
        assert_eq!(filter_from(None, false).unwrap().to_string(), "info");
        assert_eq!(filter_from(None, true).unwrap().to_string(), "debug");
        assert_eq!(
            filter_from(Some("warn"), false).unwrap().to_string(),
            "warn"
        );
        assert!(filter_from(Some("info,foo[bar=\"unterminated"), false).is_err());
    }
    #[test]
    fn rotation_retains_five_in_order() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(LOG_FILE_NAME);
        let w = RotatingWriter::new(p.clone(), 3).unwrap();
        for x in b'a'..=b'f' {
            w.commit(&[x, x, x]).unwrap();
        }
        assert_eq!(fs::read(&p).unwrap(), b"fff");
        for (n, x) in [(1, b'e'), (2, b'd'), (3, b'c'), (4, b'b')] {
            assert_eq!(fs::read(generation(&p, n)).unwrap(), vec![x; 3]);
        }
    }
    #[test]
    fn concurrent_records_are_whole_and_bounded() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(LOG_FILE_NAME);
        let w = RotatingWriter::new(p.clone(), 128).unwrap();
        let joins: Vec<_> = (0..8)
            .map(|n| {
                let w = w.clone();
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        w.commit(format!("thread-{n}\n").as_bytes()).unwrap();
                    }
                })
            })
            .collect();
        for j in joins {
            j.join().unwrap();
        }
        for n in 0..=4 {
            let p = if n == 0 { p.clone() } else { generation(&p, n) };
            if p.exists() {
                assert!(String::from_utf8(fs::read(p).unwrap())
                    .unwrap()
                    .lines()
                    .all(|l| l.starts_with("thread-")));
            }
        }
        assert!(!generation(&p, 5).exists());
    }
    #[test]
    fn unavailable_directory_is_an_error_not_a_panic() {
        let d = tempfile::tempdir().unwrap();
        let parent = d.path().join("file");
        fs::write(&parent, b"x").unwrap();
        assert!(RotatingWriter::new(parent.join("log"), 2).is_err());
    }
}
