//! Typed progress events an exporter run reports while it works.
//!
//! Log lines ([`crate::LogSink`]) are for people. Progress events are for
//! whatever draws a progress bar: the desktop app renders them, and nothing
//! has to read counts back out of prose. Every event names the stage it
//! belongs to and carries the counts that stage has, so a caller can match
//! on the variant and use the fields without parsing anything.
//!
//! The events are emitted from the shared write layer (`message-ir-format`'s
//! `ExportWriter`, write queue, and attachment stager) and from the few
//! exporter-specific loops that have progress worth showing (iMessage's
//! message stream and its backup-decrypt setup steps).

use std::fmt;
use std::sync::Arc;

use crate::attachment_jobs::AttachmentProgress;

/// One progress report from an exporter run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// A setup step before any message is read, such as decrypting an iOS
    /// backup or caching chat tables. `step` of `total`, with a short label
    /// for the person watching ("Deriving backup keys").
    Setup {
        /// What this step does, without trailing punctuation.
        label: String,
        /// One-based index of this step.
        step: usize,
        /// How many setup steps this group has.
        total: usize,
    },
    /// Messages read from the backup so far.
    Parse {
        /// Messages read.
        done: usize,
        /// Messages the backup holds, or 0 when unknown.
        total: usize,
    },
    /// Attachment files staged so far, and the bytes they add up to.
    Attachments {
        /// Attachment jobs finished.
        done: usize,
        /// Attachment jobs in the run.
        total: usize,
        /// Bytes written so far.
        bytes_done: u64,
        /// Known byte total. Grows when a file had no size hint.
        bytes_total: u64,
    },
    /// Conversation files written so far.
    Prepare {
        /// Conversation files written (or skipped on a resumed run).
        done: usize,
        /// Conversation files the run will write.
        total: usize,
    },
    /// Attachments converted or compressed so far by the media pass.
    Media {
        /// Files finished.
        done: usize,
        /// Files the pass covers.
        total: usize,
    },
}

impl From<AttachmentProgress> for ProgressEvent {
    fn from(progress: AttachmentProgress) -> Self {
        Self::Attachments {
            done: progress.done,
            total: progress.total,
            bytes_done: progress.bytes_done,
            bytes_total: progress.bytes_total,
        }
    }
}

/// Callback for typed progress events. The desktop app sets one; a run
/// without a sink reports nothing, since there is no bar to move.
#[derive(Clone)]
pub struct ProgressSink(Arc<dyn Fn(ProgressEvent) + Send + Sync>);

impl ProgressSink {
    /// Wrap a callback that receives one event at a time.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(ProgressEvent) + Send + Sync + 'static,
    {
        Self(Arc::new(f))
    }

    /// Send one event to the callback.
    pub fn emit(&self, event: ProgressEvent) {
        (self.0)(event);
    }
}

impl fmt::Debug for ProgressSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProgressSink")
    }
}

/// Send a progress event to `sink` when one is set. Unlike log lines, there
/// is no fallback: a run with no sink has nothing to draw.
pub fn emit_progress(sink: Option<&ProgressSink>, event: ProgressEvent) {
    if let Some(sink) = sink {
        sink.emit(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A sink that records every event it receives, for tests.
    pub(crate) fn recording_sink() -> (ProgressSink, Arc<Mutex<Vec<ProgressEvent>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_clone = Arc::clone(&seen);
        let sink = ProgressSink::new(move |event| seen_clone.lock().unwrap().push(event));
        (sink, seen)
    }

    #[test]
    fn emit_progress_reaches_the_sink_and_is_a_no_op_without_one() {
        let (sink, seen) = recording_sink();
        emit_progress(Some(&sink), ProgressEvent::Parse { done: 5, total: 10 });
        emit_progress(None, ProgressEvent::Parse { done: 6, total: 10 });
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [ProgressEvent::Parse { done: 5, total: 10 }]
        );
    }

    #[test]
    fn attachment_progress_maps_onto_the_attachments_event() {
        let event = ProgressEvent::from(AttachmentProgress {
            done: 2,
            total: 3,
            bytes_done: 100,
            bytes_total: 500,
        });
        assert_eq!(
            event,
            ProgressEvent::Attachments {
                done: 2,
                total: 3,
                bytes_done: 100,
                bytes_total: 500,
            }
        );
    }
}
