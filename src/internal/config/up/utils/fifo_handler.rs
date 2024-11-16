use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use tempfile::TempDir;

use crate::internal::env::tmpdir_cleanup_prefix;

#[derive(Debug)]
struct TempFifo {
    path: PathBuf,
    _temp_dir: TempDir, // Keeps cleanup guard
}

impl TempFifo {
    pub fn new() -> std::io::Result<Self> {
        // Create temporary directory with restricted permissions
        let temp_dir = tempfile::Builder::new()
            .prefix(&tmpdir_cleanup_prefix("env"))
            .rand_bytes(12)
            .tempdir()?;

        // Set directory permissions to 700 (rwx------)
        std::fs::set_permissions(temp_dir.path(), std::fs::Permissions::from_mode(0o700))?;

        // Create FIFO inside the temp directory
        let path = temp_dir.path().join("env.fifo");

        // Create FIFO with restrictive permissions (0600)
        match mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR) {
            Ok(_) => Ok(Self {
                path,
                _temp_dir: temp_dir,
            }),
            Err(err) => {
                // Convert nix error to io::Error
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create FIFO: {}", err),
                ))
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

// Implement Drop to ensure cleanup even if tempdir's drop somehow fails
impl Drop for TempFifo {
    fn drop(&mut self) {
        // We try to remove the FIFO explicitly, but don't worry if it fails
        // as the tempdir deletion will clean it up anyway
        let _ = std::fs::remove_file(&self.path);
    }
}

pub struct FifoHandler {
    path: String,
    fifo: Option<TempFifo>,
    storage: Arc<Mutex<Vec<String>>>,
    stop_signal: Sender<()>,
}

impl FifoHandler {
    pub fn new() -> std::io::Result<Self> {
        let fifo = TempFifo::new()?;

        Self::build(fifo.path().to_string_lossy().to_string(), Some(fifo))
    }

    // pub fn with_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
    // let path = path.as_ref();

    // // Create the FIFO if it doesn't exist
    // if !path.exists() {
    // mkfifo(path, Mode::S_IRWXU)?;
    // }

    // Self::build(path.to_string_lossy().to_string(), None)
    // }

    fn build(path: String, fifo: Option<TempFifo>) -> std::io::Result<Self> {
        // Shared storage for FIFO contents
        let storage = Arc::new(Mutex::new(Vec::new()));

        // Channel to signal fifo thread readiness
        let (ready_signal_transmitter, ready_signal_receiver) = mpsc::channel();

        // Channel to signal thread termination
        let (stop_signal_transmitter, stop_signal_receiver) = mpsc::channel();

        // Clone the storage for the thread
        let storage_clone = Arc::clone(&storage);

        // Spawn a background thread to read from the FIFO
        let fifo_path = path.clone();
        let _thread_handle = thread::spawn(move || {
            // Signal that the thread is ready
            let _ = ready_signal_transmitter.send(());

            loop {
                // Check for stop signal
                if stop_signal_receiver.try_recv().is_ok() {
                    break;
                }

                let fifo = match File::open(&fifo_path) {
                    Ok(fifo) => fifo,
                    Err(_err) => {
                        return;
                    }
                };
                let reader = BufReader::new(fifo);

                for line in reader.lines() {
                    // Store the line in shared storage
                    match line {
                        Ok(line) => {
                            let mut storage = storage_clone.lock().unwrap();
                            storage.push(line);
                        }
                        Err(_err) => {
                            break;
                        }
                    }
                }
            }
        });

        // Wait for the thread to be ready
        let _ = ready_signal_receiver.recv();

        Ok(Self {
            path,
            fifo,
            storage,
            stop_signal: stop_signal_transmitter,
        })
    }

    /// Returns the path to the FIFO.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the accumulated contents from the FIFO.
    pub fn lines(&self) -> Vec<String> {
        let storage = self.storage.lock().unwrap();
        storage.clone()
    }

    /// Closes the FIFO handler and terminates the background thread.
    pub fn close(&mut self) {
        // Send the stop signal
        let _ = self.stop_signal.send(());

        // Remove the FIFO file on best effort
        if let Some(fifo) = self.fifo.take() {
            drop(fifo);
        } else {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl Drop for FifoHandler {
    /// Ensure proper cleanup on drop.
    fn drop(&mut self) {
        self.close();
    }
}
