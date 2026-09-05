//! Build script. Only the window needs one: `tauri-build` bakes the app
//! manifest and the UI files into the binary. Without the `gui` feature this is
//! a no-op.

fn main() {
    #[cfg(feature = "gui")]
    tauri_build::build();
}
