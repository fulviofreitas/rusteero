//! Log capture for the storage unit tests.
//!
//! The integration tests have `tests/common`'s `capture_logs`; the library's own unit tests
//! cannot reach it, so this is the same idea in miniature. Capture is per thread
//! (`tracing::subscriber::set_default`), so it only sees events logged on the test's own thread:
//! call the synchronous store methods directly rather than through the `spawn_blocking` adapters.

use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing::subscriber::DefaultGuard;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{Layer, registry};

/// The events [`capture_logs`] recorded on the current thread: level and message, oldest first.
#[derive(Clone, Default)]
pub(crate) struct LogCapture {
    records: Arc<Mutex<Vec<(Level, String)>>>,
}

impl LogCapture {
    /// The messages of every captured event at exactly `level`.
    pub(crate) fn messages(&self, level: Level) -> Vec<String> {
        self.lock()
            .iter()
            .filter(|(record_level, _)| *record_level == level)
            .map(|(_, message)| message.clone())
            .collect()
    }

    /// How many captured events at exactly `level` have a message containing `needle`.
    pub(crate) fn count(&self, level: Level, needle: &str) -> usize {
        self.lock()
            .iter()
            .filter(|(record_level, message)| *record_level == level && message.contains(needle))
            .count()
    }

    /// Whether any captured event, at any level, has a message containing `needle`.
    pub(crate) fn mentions(&self, needle: &str) -> bool {
        self.lock()
            .iter()
            .any(|(_, message)| message.contains(needle))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(Level, String)>> {
        self.records.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }
}

struct CaptureLayer {
    records: Arc<Mutex<Vec<(Level, String)>>>,
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((*event.metadata().level(), visitor.message));
    }
}

/// Installs a capturing subscriber on the current thread. Bind the guard to a named variable
/// (`let (_guard, logs) = ...`), or the capture ends immediately.
pub(crate) fn capture_logs() -> (DefaultGuard, LogCapture) {
    let capture = LogCapture::default();
    let subscriber = registry().with(CaptureLayer {
        records: Arc::clone(&capture.records),
    });
    (tracing::subscriber::set_default(subscriber), capture)
}
