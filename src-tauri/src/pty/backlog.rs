use std::collections::VecDeque;

/// Bounded tail of a pane's raw PTY output, kept so a freshly mounted
/// `TerminalView` (after a webview reload) can replay what was on screen, and
/// so the control socket can read a pane's recent output. Memory only.
pub struct Backlog {
    buf: VecDeque<u8>,
    // Total bytes ever written; the stream offset just past the buffer's end.
    total: u64,
}

const CAPACITY: usize = 1024 * 1024;

impl Backlog {
    pub fn new() -> Self {
        Self {
            buf: VecDeque::new(),
            total: 0,
        }
    }

    /// Appends a chunk and returns the stream offset where it starts.
    pub fn push(&mut self, chunk: &[u8]) -> u64 {
        let start = self.total;
        self.total += chunk.len() as u64;
        let chunk = if chunk.len() > CAPACITY {
            &chunk[chunk.len() - CAPACITY..]
        } else {
            chunk
        };
        let overflow = (self.buf.len() + chunk.len()).saturating_sub(CAPACITY);
        self.buf.drain(..overflow);
        self.buf.extend(chunk);
        start
    }

    /// The buffered bytes and the stream offset just past them.
    pub fn snapshot(&self) -> (Vec<u8>, u64) {
        (self.buf.iter().copied().collect(), self.total)
    }
}
