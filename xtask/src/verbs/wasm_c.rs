use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use cargo_metadata::MetadataCommand;

use crate::{process, task::Step};

const ADAPTER_ENVIRONMENT: &str = "GTL_WASM_C_ADAPTER";
const COMPAT_DIRECTORY_ENVIRONMENT: &str = "GTL_WASM_C_COMPAT_DIRECTORY";
const HEADERS_DIRECTORY_ENVIRONMENT: &str = "GTL_WASM_C_HEADERS_DIRECTORY";
const TARGET_UNKNOWN: &str = "--target=wasm32-unknown-unknown";
const TARGET_FREESTANDING: &str = "--target=wasm32-freestanding";

pub(crate) fn exit_if_adapter() {
    if std::env::var_os(ADAPTER_ENVIRONMENT).is_none() {
        return;
    }

    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let status = if is_archiver(&arguments) {
        Command::new("zig").arg("ar").args(arguments).status()
    } else {
        compiler_arguments_from_environment(arguments)
            .and_then(|arguments| Command::new("zig").arg("cc").args(arguments).status())
    };
    match status {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("Tree-sitter WASM C adapter failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(crate) fn configure_step(root: &Path, step: Step) -> Result<Step> {
    let adapter = compiler_adapter().context("resolve the xtask compiler adapter")?;
    let compat_directory = root.join("crates/gtl-parser/wasm-compat");
    ensure!(
        compat_directory.join("compat.h").is_file(),
        "Tree-sitter WASM compatibility headers are missing"
    );
    let headers_directory = tree_sitter_wasm_headers(root)?;

    Ok(step
        .with_environment(ADAPTER_ENVIRONMENT, "1")
        .with_environment(
            "CC_wasm32_unknown_unknown",
            adapter.to_string_lossy().into_owned(),
        )
        .with_environment(
            "AR_wasm32_unknown_unknown",
            adapter.to_string_lossy().into_owned(),
        )
        .with_environment(
            COMPAT_DIRECTORY_ENVIRONMENT,
            compat_directory.to_string_lossy().into_owned(),
        )
        .with_environment(
            HEADERS_DIRECTORY_ENVIRONMENT,
            headers_directory.to_string_lossy().into_owned(),
        )
        .with_environment("RUSTC_WRAPPER", ""))
}

fn compiler_adapter() -> std::io::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let live_executable = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
        if live_executable.is_file() {
            return Ok(live_executable);
        }
    }
    std::env::current_exe()
}

pub(crate) fn check_parser(root: &Path) -> Result<()> {
    let step = Step::new(
        "parser-wasm-all-features",
        "cargo",
        [
            "check",
            "--locked",
            "-p",
            "gtl-parser",
            "--all-features",
            "--all-targets",
            "--target",
            "wasm32-unknown-unknown",
        ],
    )
    .with_current_directory(root);
    process::run_step(&configure_step(root, step)?)
}

fn compiler_arguments_from_environment(arguments: Vec<OsString>) -> std::io::Result<Vec<OsString>> {
    let compat_directory = environment_path(COMPAT_DIRECTORY_ENVIRONMENT)?;
    let headers_directory = environment_path(HEADERS_DIRECTORY_ENVIRONMENT)?;
    Ok(compiler_arguments(
        arguments,
        &compat_directory,
        &headers_directory,
    ))
}

fn environment_path(name: &str) -> std::io::Result<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{name} is not configured"),
        )
    })
}

fn compiler_arguments(
    arguments: Vec<OsString>,
    compat_directory: &Path,
    headers_directory: &Path,
) -> Vec<OsString> {
    let compiles_upstream_wasm_shim = arguments.iter().any(|argument| {
        let argument = argument.to_string_lossy().replace('\\', "/");
        argument.contains("/tree-sitter-language-")
            && argument.contains("/wasm/src/")
            && Path::new(&argument)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("c"))
    });
    let compiles_upstream_stdlib = arguments.iter().any(|argument| {
        let argument = argument.to_string_lossy().replace('\\', "/");
        argument.contains("/tree-sitter-language-") && argument.ends_with("/wasm/src/stdlib.c")
    });
    let mut configured = Vec::with_capacity(arguments.len() + 7);
    if !compiles_upstream_wasm_shim {
        configured.push("-include".into());
        configured.push(compat_directory.join("compat.h").into_os_string());
        configured.push("-I".into());
        configured.push(compat_directory.as_os_str().to_owned());
        configured.push("-I".into());
        configured.push(headers_directory.as_os_str().to_owned());
    }
    if compiles_upstream_stdlib {
        configured.push("-Wno-error=incompatible-pointer-types".into());
    }
    configured.extend(arguments.into_iter().map(|argument| {
        if argument == OsStr::new(TARGET_UNKNOWN) {
            TARGET_FREESTANDING.into()
        } else {
            argument
        }
    }));
    configured
}

fn is_archiver(arguments: &[OsString]) -> bool {
    arguments
        .first()
        .and_then(|argument| argument.to_str())
        .is_some_and(|argument| matches!(argument, "cq" | "crs" | "s"))
}

fn tree_sitter_wasm_headers(root: &Path) -> Result<PathBuf> {
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .current_dir(root)
        .other_options(vec!["--locked".to_owned()])
        .exec()
        .context("resolve locked Tree-sitter dependencies")?;
    let package = metadata
        .packages
        .iter()
        .find(|package| package.name == "tree-sitter-language")
        .context("locked dependencies do not contain tree-sitter-language")?;
    let manifest = package.manifest_path.clone().into_std_path_buf();
    let headers = manifest
        .parent()
        .context("tree-sitter-language manifest has no parent")?
        .join("wasm/include");
    ensure!(
        headers.join("stdint.h").is_file(),
        "tree-sitter-language does not provide its WASM compatibility headers"
    );
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(arguments: Vec<OsString>) -> Vec<String> {
        arguments
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn grammar_compilation_uses_freestanding_target_and_compatibility_headers() {
        let arguments = compiler_arguments(
            vec![
                TARGET_UNKNOWN.into(),
                "-c".into(),
                "grammar/src/scanner.c".into(),
            ],
            Path::new("/repo/compat"),
            Path::new("/cargo/tree-sitter-language/wasm/include"),
        );
        let arguments = strings(arguments);

        assert!(arguments.contains(&TARGET_FREESTANDING.to_owned()));
        assert!(!arguments.contains(&TARGET_UNKNOWN.to_owned()));
        assert!(arguments.contains(&"/repo/compat/compat.h".to_owned()));
        assert!(arguments.contains(&"/cargo/tree-sitter-language/wasm/include".to_owned()));
    }

    #[test]
    fn upstream_wasm_shim_avoids_compatibility_macro_redefinitions() {
        let arguments = compiler_arguments(
            vec![
                TARGET_UNKNOWN.into(),
                "-c".into(),
                "/cargo/tree-sitter-language-0.1.7/wasm/src/stdio.c".into(),
            ],
            Path::new("/repo/compat"),
            Path::new("/cargo/tree-sitter-language/wasm/include"),
        );
        let arguments = strings(arguments);

        assert!(arguments.contains(&TARGET_FREESTANDING.to_owned()));
        assert!(!arguments.contains(&"/repo/compat".to_owned()));
        assert!(!arguments.contains(&"/repo/compat/compat.h".to_owned()));
    }

    #[test]
    fn upstream_stdlib_allows_its_mismatched_region_pointer() {
        let arguments = compiler_arguments(
            vec![
                TARGET_UNKNOWN.into(),
                "-c".into(),
                "/cargo/tree-sitter-language-0.1.7/wasm/src/stdlib.c".into(),
            ],
            Path::new("/repo/compat"),
            Path::new("/cargo/tree-sitter-language/wasm/include"),
        );
        let arguments = strings(arguments);

        assert!(arguments.contains(&"-Wno-error=incompatible-pointer-types".to_owned()));
        assert!(!arguments.contains(&"/repo/compat".to_owned()));
        assert!(!arguments.contains(&"/repo/compat/compat.h".to_owned()));
    }

    #[test]
    fn cargo_archiver_invocations_route_to_zig_ar() {
        assert!(is_archiver(&["cq".into(), "library.a".into()]));
        assert!(is_archiver(&["s".into(), "library.a".into()]));
        assert!(!is_archiver(&["-c".into(), "source.c".into()]));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_adapter_uses_the_live_process_executable() {
        assert_eq!(
            compiler_adapter().expect("resolve compiler adapter"),
            PathBuf::from(format!("/proc/{}/exe", std::process::id()))
        );
    }
}
