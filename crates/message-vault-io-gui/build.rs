fn main() {
    // Explicit `native` so platform selection is intentional. On Windows this is
    // Fluent, on macOS Cupertino, and on Linux Qt when available (otherwise Fluent).
    // Override at compile time with SLINT_STYLE if needed.
    let config = slint_build::CompilerConfiguration::new().with_style("native".into());
    slint_build::compile_with_config("ui/app-window.slint", config).unwrap();
}
