//! Background jobs with cooperative cancel and a channel for log lines.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

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

/// Failure returned by an in-process GUI job.
#[derive(Debug, Clone)]
pub struct JobError {
    /// Technical failure detail.
    pub detail: String,
    /// Optional short banner text for the GUI.
    pub user_message: Option<String>,
}

impl JobError {
    /// Failure with a technical detail and no separate banner text.
    pub fn detail(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            user_message: None,
        }
    }

    /// Failure with a technical detail plus a short banner for the GUI.
    pub fn with_user_message(detail: impl Into<String>, user_message: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            user_message: Some(user_message.into()),
        }
    }

    /// True when the detail text contains `cancelled` (case-insensitive).
    pub fn is_cancelled(&self) -> bool {
        self.detail.to_ascii_lowercase().contains("cancelled")
    }
}

impl From<String> for JobError {
    fn from(detail: String) -> Self {
        Self::detail(detail)
    }
}

impl From<&str> for JobError {
    fn from(detail: &str) -> Self {
        Self::detail(detail.to_string())
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

#[derive(Debug, Clone)]
/// One event sent on the job channel: started, log line, finished, or error.
pub enum ProcessEvent {
    /// Job started; carries the job label.
    Started(String),
    /// Mid-run progress or warning line.
    Log(String),
    /// Job finished; carries the completion message.
    Finished(String),
    /// Job failed; carries the error detail and optional banner text.
    Error {
        /// Technical failure detail.
        detail: String,
        /// Optional short banner text for the GUI.
        user_message: Option<String>,
    },
}

impl ProcessEvent {
    /// Error that uses the same text for the log and the banner.
    pub fn error_detail(detail: impl Into<String>) -> Self {
        Self::Error {
            detail: detail.into(),
            user_message: None,
        }
    }
}

#[derive(Debug, Clone)]
/// Cancel flag shared with a running in-process job.
pub struct ProcessControl {
    cancel: CancelFlag,
}

impl Default for ProcessControl {
    fn default() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ProcessControl {
    /// Shared cancel flag for in-process jobs. Reset when a new job starts.
    pub fn cancel_flag(&self) -> CancelFlag {
        Arc::clone(&self.cancel)
    }

    /// Ask the running job to stop. Always succeeds; the job checks the flag.
    ///
    /// # Errors
    ///
    /// The `Result` is for a stable GUI API; this method currently always returns `Ok`.
    pub fn cancel(&self) -> Result<(), String> {
        self.cancel.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Clear the cancel flag before a new job starts.
    fn begin_job(&self) {
        self.cancel.store(false, Ordering::Relaxed);
    }
}

/// Run an in-process job on a background thread; send events on `tx`.
///
/// The job receives the shared cancel flag and a log sender. On success it should
/// return `Ok(())`; on failure `Err(JobError)`. Cancelled jobs typically return
/// an error whose detail contains `"cancelled"`.
pub fn spawn_job<F>(control: ProcessControl, tx: mpsc::Sender<ProcessEvent>, label: String, job: F)
where
    F: FnOnce(CancelFlag, mpsc::Sender<ProcessEvent>) -> Result<(), JobError> + Send + 'static,
{
    control.begin_job();
    let cancel = control.cancel_flag();
    thread::spawn(move || {
        let _ = tx.send(ProcessEvent::Started(label));
        match job(cancel, tx.clone()) {
            Ok(()) => {
                let _ = tx.send(ProcessEvent::Finished(
                    "Completed successfully.".to_string(),
                ));
            }
            Err(error) => {
                if error.is_cancelled() {
                    let _ = tx.send(ProcessEvent::Finished("Cancelled.".to_string()));
                } else {
                    let _ = tx.send(ProcessEvent::Error {
                        detail: error.detail,
                        user_message: error.user_message,
                    });
                }
            }
        }
    });
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

    #[test]
    fn cancel_sets_flag_without_child() {
        let control = ProcessControl::default();
        assert!(!control.cancel_flag().load(Ordering::Relaxed));
        control.cancel().unwrap();
        assert!(control.cancel_flag().load(Ordering::Relaxed));
    }
}
