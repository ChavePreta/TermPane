pub mod io;
pub mod spawn;

use parking_lot::Mutex;
use portable_pty::{Child, MasterPty, PtySize};
use std::io::Write;
use std::sync::Arc;
use std::thread::JoinHandle;

pub use spawn::{spawn_shell, SpawnResult};

/// Live handle for a PTY: master + child + writer + reader-thread.
pub struct PtyHandle {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    pub shell_pid: Option<u32>,
    _reader_thread: Mutex<Option<JoinHandle<()>>>,
}

impl PtyHandle {
    pub fn new(
        master: Box<dyn MasterPty + Send>,
        writer: Box<dyn Write + Send>,
        child: Box<dyn Child + Send + Sync>,
        shell_pid: Option<u32>,
        reader_thread: JoinHandle<()>,
    ) -> Self {
        Self {
            master: Mutex::new(master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            shell_pid,
            _reader_thread: Mutex::new(Some(reader_thread)),
        }
    }

    pub fn write_all(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut w = self.writer.lock();
        w.write_all(bytes)?;
        w.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        self.master
            .lock()
            .resize(size)
            .map_err(|e| anyhow::anyhow!("failed to resize PTY: {e}"))
    }

    /// PID of the leader of the foreground process group on the TTY (Unix only).
    pub fn foreground_pgid(&self) -> Option<i32> {
        self.master.lock().process_group_leader()
    }

    pub fn kill(&self) {
        if let Err(e) = self.child.lock().kill() {
            tracing::debug!("kill child failed (probably already dead): {e}");
        }
        // The reader thread exits on its own when the master closes (EOF/error).
    }
}

pub type SharedPty = Arc<PtyHandle>;
