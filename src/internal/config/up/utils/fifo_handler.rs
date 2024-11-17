use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Arc;
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

pub struct FifoReader {
    fifo_path: PathBuf,
    _temp_fifo: Option<TempFifo>,
    stop_signal: Arc<AtomicBool>,
    reader_handle: Option<thread::JoinHandle<std::io::Result<()>>>,
    receiver: Receiver<String>,
    lines: Vec<String>,
}

impl FifoReader {
    /// Create a new FifoReader with a temporary FIFO.
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

    /// Create a new FifoReader with the given FIFO path.
    fn build<P: AsRef<Path>>(fifo_path: P, temp_fifo: Option<TempFifo>) -> std::io::Result<Self> {
        let fifo_path = fifo_path.as_ref().to_path_buf();
        let stop_signal = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();

        let reader_handle = Some(Self::spawn_reader(
            fifo_path.clone(),
            stop_signal.clone(),
            sender,
        ));

        Ok(Self {
            fifo_path,
            _temp_fifo: temp_fifo,
            stop_signal,
            reader_handle,
            receiver,
            lines: Vec::new(),
        })
    }

    /// Returns the path to the FIFO.
    pub fn path(&self) -> &Path {
        &self.fifo_path
    }

    /// Open the FIFO for reading.
    fn open_fifo_in(path: &Path) -> std::io::Result<File> {
        OpenOptions::new().read(true).open(path)
    }

    /// Open the FIFO for writing.
    fn open_fifo_out(path: &Path) -> std::io::Result<File> {
        OpenOptions::new()
            .custom_flags(nix::libc::O_NONBLOCK)
            .write(true)
            .open(path)
    }

    /// Spawn a reader thread that reads from the FIFO until it is closed.
    fn spawn_reader(
        fifo_path: PathBuf,
        stop_signal: Arc<AtomicBool>,
        sender: Sender<String>,
    ) -> thread::JoinHandle<std::io::Result<()>> {
        thread::spawn(move || {
            while !stop_signal.load(Ordering::Relaxed) {
                match Self::read_fifo_until_closed(&fifo_path, &stop_signal, &sender) {
                    Ok(()) => {}
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
            Ok(())
        })
    }

    /// Check if the error is recoverable and the operation can be retried.
    fn is_recoverable_error(e: &std::io::Error) -> bool {
        use std::io::ErrorKind::*;
        matches!(
            e.kind(),
            NotFound
                | PermissionDenied
                | ConnectionReset
                | ConnectionAborted
                | BrokenPipe
                | WouldBlock
        )
    }

    /// Read from the FIFO until it is closed.
    fn read_fifo_until_closed(
        fifo_path: &Path,
        stop_signal: &AtomicBool,
        sender: &Sender<String>,
    ) -> std::io::Result<()> {
        let file = match Self::open_fifo_in(fifo_path) {
            Ok(f) => f,
            Err(e) if Self::is_recoverable_error(&e) => return Ok(()), // Allow retry
            Err(e) => return Err(e),
        };

        let mut reader = BufReader::new(file);
        let mut buffer = String::new();

        loop {
            if stop_signal.load(Ordering::Relaxed) {
                break;
            }

            buffer.clear();
            match reader.read_line(&mut buffer) {
                Ok(0) => {
                    // EOF reached, try again
                }
                Ok(_) => {
                    if let Err(e) = sender.send(buffer.to_string()) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Failed to send data: {}", e),
                        ));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available right now, try again
                }
                Err(e) if Self::is_recoverable_error(&e) => return Ok(()), // Allow retry
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    /// Flush any pending messages from the FIFO.
    fn flush(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            // Remove one \n from the end of the message if any
            let msg = if msg.ends_with('\n') {
                msg[..msg.len() - 1].to_string()
            } else {
                msg
            };

            self.lines.push(msg);
        }
    }

    /// Stop the reader thread and return all the lines collected so far.
    pub fn stop(&mut self) -> std::io::Result<Vec<String>> {
        // Send the stop signal to the reader thread
        self.stop_signal.store(true, Ordering::Relaxed);

        // Collect all buffered messages
        self.flush();

        // Wait for the reader thread to finish and check for any errors
        if let Some(handle) = self.reader_handle.take() {
            // Since we are doing blocking I/O in the reader thread, we need to
            // open the FIFO for writing to make sure it gets unblocked.
            // This open is unblocking, and we do not mind if there is an error.
            let _ = Self::open_fifo_out(&self.fifo_path);

            // Now we can join the reader thread
            handle.join().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "Reader thread panicked")
            })??;
        }

        // Collect any final messages that might have been sent while joining
        self.flush();

        // Return the collected messages
        Ok(self.lines.clone())
    }

    // /// Try to read any available messages from the FIFO and return all the lines
    // /// collected so far.
    // pub fn try_read(&mut self) -> Vec<String> {
    // if self.stop_signal.load(Ordering::Relaxed) {
    // self.flush();
    // }
    // self.lines.clone()
    // }

    // /// Returns the accumulated contents from the FIFO.
    // pub fn lines(&self) -> Vec<String> {
    // self.lines.clone()
    // }
}

impl Drop for FifoReader {
    /// Ensure proper cleanup on drop.
    fn drop(&mut self) {
        self.stop().ok();
    }
}
