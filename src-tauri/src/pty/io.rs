use crate::model::PaneId;
use base64::Engine;
use std::io::Read;
use std::thread;
use tauri::{AppHandle, Emitter};

/// Spawns an OS thread that reads from the (sync) reader and emits chunks to
/// the frontend as Tauri events `pty:output:<pane_id>` with base64 payload.
/// When the read returns EOF/error, emits `pty:exit:<pane_id>`.
pub fn start_reader_thread(
    app: AppHandle,
    pane_id: PaneId,
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
                        let b64 =
                            base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                        if let Err(e) = app.emit(&output_event, b64) {
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
