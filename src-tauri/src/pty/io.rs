use super::backlog::Backlog;
use crate::model::PaneId;
use base64::Engine;
use parking_lot::Mutex;
use serde::Serialize;
use std::io::Read;
use std::sync::Arc;
use std::thread;
use tauri::{AppHandle, Emitter};

/// One PTY output chunk. `o` is the stream offset of the chunk's first byte, so
/// a view that replays the backlog can drop chunks it has already seen.
#[derive(Serialize, Clone)]
struct OutputChunk {
    o: u64,
    d: String,
}

/// Spawns an OS thread that reads from the (sync) reader, appends each chunk to
/// the pane's backlog, and emits it to the frontend as `pty:output:<pane_id>`.
/// When the read returns EOF/error, emits `pty:exit:<pane_id>`.
pub fn start_reader_thread(
    app: AppHandle,
    pane_id: PaneId,
    backlog: Arc<Mutex<Backlog>>,
    mut reader: Box<dyn Read + Send>,
) -> thread::JoinHandle<()> {
    let output_event = format!("pty:output:{}", pane_id);
    let exit_event = format!("pty:exit:{}", pane_id);

    thread::Builder::new()
        .name(format!("pty-reader-{}", pane_id))
        .spawn(move || {
            let mut buf = vec![0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        tracing::debug!("PTY {} reader EOF", pane_id);
                        break;
                    }
                    Ok(n) => {
                        let o = backlog.lock().push(&buf[..n]);
                        let d = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                        if let Err(e) = app.emit(&output_event, OutputChunk { o, d }) {
                            tracing::warn!("emit {} failed: {}", output_event, e);
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!("PTY {} reader err: {}", pane_id, e);
                        break;
                    }
                }
            }
            let _ = app.emit(&exit_event, ());
        })
        .expect("failed to create reader thread")
}
