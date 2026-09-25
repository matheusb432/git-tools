// Generating the context here writes the compressed frontend assets before the crate compiles.
// `tauri::generate_context!` writes new assets during compilation instead, which leaves them newer
// than Cargo's record of that compile and forces a second rebuild after every frontend change.
fn main() -> anyhow::Result<()> {
    tauri_build::try_build(
        tauri_build::Attributes::new().codegen(tauri_build::CodegenContext::new()),
    )
}
