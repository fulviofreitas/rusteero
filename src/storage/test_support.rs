//! Log capture for the storage unit tests.
//!
//! The integration tests have `tests/common`'s `capture_logs`; the library's own unit tests
//! cannot reach it, so this is the same idea in miniature. Capture is per thread
//! (`tracing::subscriber::set_default`), so it sees the events logged on the test's own thread,
//! and, through the `spawn_blocking` adapters in `storage/mod.rs`, the ones a store logs on the
//! blocking pool, because those forward the calling task's dispatcher.
//!
//! An event is recorded as its message plus every structured field as `name=value`.
//! [`LogCapture::mentions`] scans both, so a leak assertion also catches a credential that
//! reaches the log through a field (`tracing::warn!(error = %err, ..)`) and not the message;
//! [`LogCapture::messages`] and [`LogCapture::count`] look at messages only.

use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing::subscriber::DefaultGuard;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{Layer, registry};

/// One captured event: its level, its message and its other fields as `name=value`.
#[derive(Clone)]
struct Record {
    level: Level,
    message: String,
    fields: Vec<String>,
}

/// The events [`capture_logs`] recorded on the current thread, oldest first.
#[derive(Clone, Default)]
pub(crate) struct LogCapture {
    records: Arc<Mutex<Vec<Record>>>,
}

impl LogCapture {
    /// The messages of every captured event at exactly `level`.
    pub(crate) fn messages(&self, level: Level) -> Vec<String> {
        self.lock()
            .iter()
            .filter(|record| record.level == level)
            .map(|record| record.message.clone())
            .collect()
    }

    /// How many captured events at exactly `level` have a message containing `needle`.
    pub(crate) fn count(&self, level: Level, needle: &str) -> usize {
        self.lock()
            .iter()
            .filter(|record| record.level == level && record.message.contains(needle))
            .count()
    }

    /// Whether any captured event, at any level, has a message or a field containing `needle`.
    pub(crate) fn mentions(&self, needle: &str) -> bool {
        self.lock().iter().any(|record| {
            record.message.contains(needle)
                || record.fields.iter().any(|field| field.contains(needle))
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Record>> {
        self.records.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Collects an event's message and its other fields.
#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: Vec<String>,
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            value.clone_into(&mut self.message);
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }

    // Every other value type (numbers, booleans, `%` and `?` wrappers, errors) reaches the
    // visitor through this method by default.
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }
}

struct CaptureLayer {
    records: Arc<Mutex<Vec<Record>>>,
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Record {
                level: *event.metadata().level(),
                message: visitor.message,
                fields: visitor.fields,
            });
    }
}

/// Registers, once per test binary, an always-live dispatcher that is never installed anywhere.
///
/// With it alive, `tracing-core` never decides a callsite's interest against a single thread's
/// default alone, so a scoped subscriber installed with `set_default`/`with_default` sees every
/// event it enables even when parallel tests on other threads hit the same callsite first. Call
/// it before installing any scoped subscriber in a unit test.
pub(crate) fn register_anchor_dispatcher() {
    static ANCHOR: std::sync::OnceLock<tracing::Dispatch> = std::sync::OnceLock::new();
    ANCHOR.get_or_init(|| tracing::Dispatch::new(registry()));
}

/// Installs a capturing subscriber on the current thread. Bind the guard to a named variable
/// (`let (_guard, logs) = ...`), or the capture ends immediately.
///
/// Registers the anchor dispatcher first ([`register_anchor_dispatcher`]), so a parallel unit
/// test that hit a store's log line before this capture existed cannot hide it from the capture.
pub(crate) fn capture_logs() -> (DefaultGuard, LogCapture) {
    register_anchor_dispatcher();
    let capture = LogCapture::default();
    let subscriber = registry().with(CaptureLayer {
        records: Arc::clone(&capture.records),
    });
    (tracing::subscriber::set_default(subscriber), capture)
}

#[cfg(test)]
mod tests {
    use tracing::Level;

    use super::capture_logs;

    #[test]
    fn mentions_sees_structured_fields_of_every_kind() {
        let (_guard, logs) = capture_logs();
        let cause = std::io::Error::other("field_cause");

        tracing::warn!(token = "str_field_secret", "plain message");
        tracing::warn!(path = ?"debug_field_secret", "plain message");
        tracing::warn!(error = %cause, "plain message");
        tracing::warn!(bytes = 4242_u64, "plain message");

        for needle in [
            "str_field_secret",
            "debug_field_secret",
            "field_cause",
            "4242",
        ] {
            assert!(logs.mentions(needle), "{needle} is in a field");
        }
        assert_eq!(logs.count(Level::WARN, "plain message"), 4);
        assert!(
            logs.messages(Level::WARN)
                .iter()
                .all(|message| message == "plain message"),
            "messages() carries the message only"
        );
    }
}
