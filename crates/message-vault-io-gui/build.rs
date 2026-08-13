//! Compile the Slint UI files into Rust during `cargo build`.

/// Compile `ui/app-window.slint` with the platform-native widget style.
fn main() {
    // `native` picks the platform widget style on purpose.
    // Windows uses Fluent, macOS uses Cupertino, and Linux uses Qt when it is
    // available (otherwise Fluent). Override at compile time with `SLINT_STYLE`.
    let config = slint_build::CompilerConfiguration::new().with_style("native".into());
    slint_build::compile_with_config("ui/app-window.slint", config).unwrap();
}
