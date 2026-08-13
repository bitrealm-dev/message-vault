//! Copy Tauri config and icons into the build so the desktop binary can
//! create its window.

/// Run the Tauri build helper for this crate.
fn main() {
    tauri_build::build();
}
