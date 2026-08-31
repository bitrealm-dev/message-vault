//! Cooperative cancel flags and a log-line sink shared by exporter runs.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Shared cancel flag for cooperative in-process jobs.
pub type CancelFlag = Arc<AtomicBool>;

/// Callback for mid-run progress / warning lines (GUI streams these; CLI leaves unset).
#[derive(Clone)]
pub struct LogSink(Arc<dyn Fn(&str) + Send + Sync>);

impl LogSink {
    /// Wrap a callback that receives one log line at a time.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        Self(Arc::new(f))
    }

    /// Send one log line to the callback.
    pub fn emit(&self, line: &str) {
        (self.0)(line);
    }
}

impl fmt::Debug for LogSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LogSink")
    }
}

/// Send a log line to `sink` when set. Otherwise print to stderr (CLI default).
pub fn emit_log(sink: Option<&LogSink>, line: impl AsRef<str>) {
    let line = line.as_ref();
    match sink {
        Some(sink) => sink.emit(line),
        None => eprintln!("{line}"),
    }
}

/// Whether cancel has been requested.
pub fn is_cancelled(cancel: Option<&CancelFlag>) -> bool {
    cancel
        .map(|flag| flag.load(Ordering::Relaxed))
        .unwrap_or(false)
}

/// Return `Err("cancelled")` if cancel was requested.
///
/// # Errors
///
/// Returns `"cancelled"` when the flag is set.
pub fn check_cancel(cancel: Option<&CancelFlag>) -> Result<(), &'static str> {
    if is_cancelled(cancel) {
        Err("cancelled")
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn emit_log_uses_sink_when_set() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let lines_clone = Arc::clone(&lines);
        let sink = LogSink::new(move |line| {
            lines_clone.lock().unwrap().push(line.to_string());
        });
        emit_log(Some(&sink), "hello");
        assert_eq!(lines.lock().unwrap().as_slice(), ["hello"]);
    }
}
