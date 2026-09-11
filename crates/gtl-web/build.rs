#[cfg(feature = "artifact")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string("src/artifact.js")?;
    let output = minifier::js::minify(&source)
        .map_err(|message| format!("minify src/artifact.js: {message}"))?
        .to_string();
    let directory =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?);
    std::fs::write(directory.join("artifact.min.js"), output)?;
    println!("cargo:rerun-if-changed=src/artifact.js");
    Ok(())
}

#[cfg(not(feature = "artifact"))]
fn main() {}
