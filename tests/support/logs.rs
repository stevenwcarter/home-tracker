//! Captures formatted log lines in a test. Test-only code: failures panic.

use std::io;
use std::sync::{Arc, Mutex};

use tracing::subscriber::{self, DefaultGuard};

/// Formatted log lines emitted on this thread while the capture's guard
/// lives. `#[tokio::test]` runs every task on the test's thread, so the
/// handlers' and background tasks' events land here.
#[derive(Clone, Default)]
pub struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
    pub fn capture() -> (Self, DefaultGuard) {
        let logs = Self::default();
        let writer = logs.clone();
        let collector = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        (logs, subscriber::set_default(collector))
    }

    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl io::Write for Logs {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
