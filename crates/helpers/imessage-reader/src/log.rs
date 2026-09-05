//! Events go to stdout, one JSON line each. Log lines are events too, so the
//! app sees them in the order they happened relative to the messages.

use std::io::Write;

use imessage_reader_protocol::Event;

/// Write one event line to stdout and flush it, so the app reads it now
/// rather than when the buffer fills.
pub(crate) fn emit(event: &Event) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // A failed write means the app has gone away; there is nobody left to
    // tell, and the next read of stdin ends the process.
    let _ = serde_json::to_writer(&mut out, event);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

/// Send one log line to the app.
pub(crate) fn emit_log(line: impl AsRef<str>) {
    emit(&Event::Log {
        line: line.as_ref().to_string(),
    });
}
