use std::{
    ffi::{OsStr, OsString},
    fmt::Write as _,
    fs::{self, File},
    io::{self, Read as _},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use cargo_metadata::MetadataCommand;
use sha2::{Digest, Sha256};

use crate::{process, task::Step};

const ADAPTER_ENVIRONMENT: &str = "GTL_WASM_C_ADAPTER";
const COMPAT_DIRECTORY_ENVIRONMENT: &str = "GTL_WASM_C_COMPAT_DIRECTORY";
const HEADERS_DIRECTORY_ENVIRONMENT: &str = "GTL_WASM_C_HEADERS_DIRECTORY";
const TARGET_UNKNOWN: &str = "--target=wasm32-unknown-unknown";
const TARGET_FREESTANDING: &str = "--target=wasm32-freestanding";
const HEADER_FILES_MAX: usize = 64;
const HEADER_BYTES_MAX: u64 = 1024 * 1024;

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
    let compat_directory = root
        .join("crates/gtl-parser/wasm-compat")
        .canonicalize()
        .context("resolve Tree-sitter WASM compatibility headers")?;
    ensure!(
        compat_directory.join("compat.h").is_file(),
        "Tree-sitter WASM compatibility headers are missing"
    );
    let headers_directory = tree_sitter_wasm_headers(root)?;
    let adapter = compiler_adapter(root, &compat_directory, &headers_directory)?;

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

fn compiler_adapter(
    root: &Path,
    compat_directory: &Path,
    headers_directory: &Path,
) -> Result<PathBuf> {
    let zig_version = process::capture_bytes("zig-version", "zig", &["version"])?;
    let fingerprint = adapter_fingerprint(compat_directory, headers_directory, &zig_version)?;
    let cache = super::cargo_target_directory(root)?.join("wasm-c-adapters");
    stage_compiler_adapter(&live_executable()?, &cache, &fingerprint)
        .context("stage the Tree-sitter WASM C adapter")
}

fn live_executable() -> io::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let live_executable = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
        if live_executable.is_file() {
            return Ok(live_executable);
        }
    }
    std::env::current_exe()
}

fn adapter_fingerprint(
    compat_directory: &Path,
    headers_directory: &Path,
    zig_version: &[u8],
) -> Result<String> {
    let mut hash = Sha256::new();
    hash_adapter_input(&mut hash, include_bytes!("wasm_c.rs"));
    hash_adapter_input(&mut hash, std::env::consts::OS.as_bytes());
    hash_adapter_input(&mut hash, std::env::consts::ARCH.as_bytes());
    hash_adapter_input(&mut hash, zig_version);
    for directory in [compat_directory, headers_directory] {
        hash_adapter_input(&mut hash, directory.as_os_str().as_encoded_bytes());
        let mut headers = fs::read_dir(directory)?
            .take(HEADER_FILES_MAX + 1)
            .collect::<io::Result<Vec<_>>>()?;
        ensure!(
            headers.len() <= HEADER_FILES_MAX,
            "too many WASM compatibility headers in {}",
            directory.display()
        );
        headers.sort_by_key(fs::DirEntry::file_name);
        for header in headers {
            ensure!(
                header.file_type()?.is_file(),
                "WASM compatibility header is not a regular file: {}",
                header.path().display()
            );
            let mut contents = Vec::new();
            File::open(header.path())?
                .take(HEADER_BYTES_MAX + 1)
                .read_to_end(&mut contents)?;
            ensure!(
                contents.len() as u64 <= HEADER_BYTES_MAX,
                "WASM compatibility header is too large: {}",
                header.path().display()
            );
            hash_adapter_input(&mut hash, header.file_name().as_encoded_bytes());
            hash_adapter_input(&mut hash, &contents);
        }
    }
    let mut fingerprint = String::with_capacity(64);
    for byte in hash.finalize() {
        write!(&mut fingerprint, "{byte:02x}").context("encode WASM C adapter fingerprint")?;
    }
    Ok(fingerprint)
}

fn hash_adapter_input(hash: &mut Sha256, input: &[u8]) {
    hash.update(input.len().to_le_bytes());
    hash.update(input);
}

fn stage_compiler_adapter(
    executable: &Path,
    cache: &Path,
    fingerprint: &str,
) -> io::Result<PathBuf> {
    let directory = cache.join(fingerprint);
    let adapter = directory.join(format!("xtask{}", std::env::consts::EXE_SUFFIX));
    if adapter.is_file() {
        return Ok(adapter);
    }

    fs::create_dir_all(&directory)?;
    let snapshot = tempfile::NamedTempFile::new_in(&directory)?;
    // An immutable snapshot survives concurrent xtask relinks without putting a PID in CC or AR.
    fs::copy(executable, snapshot.path())?;
    match snapshot.persist_noclobber(&adapter) {
        Ok(_) => Ok(adapter),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(adapter),
        Err(error) => Err(error.error),
    }
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
            "--jobs",
            "1",
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
        &std::env::current_dir()?,
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
    current_directory: &Path,
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
    let mut configured = Vec::with_capacity(arguments.len() + 9);
    // Zig's debug instrumentation makes large generated grammars exceed workstation memory.
    configured.push("-fno-sanitize=undefined".into());
    configured.push("-g0".into());
    configured.push("-O3".into());
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
    let mut expects_path = false;
    for argument in arguments {
        if expects_path {
            configured.push(absolute_argument_path(&argument, current_directory));
            expects_path = false;
            continue;
        }
        if is_profile_argument(&argument) {
            continue;
        }
        if argument == OsStr::new(TARGET_UNKNOWN) {
            configured.push(TARGET_FREESTANDING.into());
        } else if matches!(
            argument.to_str(),
            Some("-I" | "-isystem" | "-iquote" | "-include" | "-imacros")
        ) {
            expects_path = true;
            configured.push(argument);
        } else if let Some(directory) = argument.to_str().and_then(|arg| arg.strip_prefix("-I")) {
            let mut include = OsString::from("-I");
            include.push(absolute_argument_path(
                OsStr::new(directory),
                current_directory,
            ));
            configured.push(include);
        } else if Path::new(&argument)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("c"))
            && !argument.to_string_lossy().starts_with('-')
        {
            // Zig otherwise shares one manifest between different grammars named src/parser.c.
            configured.push(absolute_argument_path(&argument, current_directory));
        } else {
            configured.push(argument);
        }
    }
    configured
}

fn absolute_argument_path(argument: &OsStr, current_directory: &Path) -> OsString {
    current_directory.join(argument).into_os_string()
}

fn is_profile_argument(argument: &OsStr) -> bool {
    matches!(
        argument.to_str(),
        Some(
            "-O" | "-O0"
                | "-O1"
                | "-O2"
                | "-O3"
                | "-O4"
                | "-Og"
                | "-Os"
                | "-Oz"
                | "-Ofast"
                | "-g"
                | "-g0"
                | "-g1"
                | "-g2"
                | "-g3"
                | "-ggdb"
                | "-ggdb0"
                | "-ggdb1"
                | "-ggdb2"
                | "-ggdb3"
                | "-gdwarf-2"
                | "-gdwarf-3"
                | "-gdwarf-4"
                | "-gdwarf-5"
                | "-gline-tables-only"
                | "-gline-directives-only"
                | "-fomit-frame-pointer"
                | "-fno-omit-frame-pointer"
        )
    )
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
            Path::new("/cargo/grammar"),
        );
        let arguments = strings(arguments);

        assert!(arguments.contains(&TARGET_FREESTANDING.to_owned()));
        assert!(!arguments.contains(&TARGET_UNKNOWN.to_owned()));
        assert!(arguments.contains(&"-fno-sanitize=undefined".to_owned()));
        assert!(arguments.contains(&"-g0".to_owned()));
        assert!(arguments.contains(&"-O3".to_owned()));
        assert!(arguments.contains(&"/repo/compat/compat.h".to_owned()));
        assert!(arguments.contains(&"/cargo/tree-sitter-language/wasm/include".to_owned()));
    }

    #[test]
    fn debug_and_release_grammars_use_identical_compiler_flags() {
        let arguments = |profile: &[&str]| {
            compiler_arguments(
                profile
                    .iter()
                    .copied()
                    .chain([TARGET_UNKNOWN, "-I", "src", "-c", "src/parser.c"])
                    .map(OsString::from)
                    .collect(),
                Path::new("/repo/compat"),
                Path::new("/cargo/tree-sitter-language/wasm/include"),
                Path::new("/cargo/grammar"),
            )
        };

        let release = arguments(&["-O3"]);
        for profile in [
            &["-O0", "-g", "-gdwarf-4", "-fno-omit-frame-pointer"][..],
            &["-Oz", "-g1", "-fomit-frame-pointer"][..],
        ] {
            assert_eq!(arguments(profile), release);
        }
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
            Path::new("/cargo/grammar"),
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
            Path::new("/cargo/grammar"),
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

    #[test]
    fn grammar_sources_and_include_directories_are_absolute() {
        let arguments = |directory: &str| {
            strings(compiler_arguments(
                [
                    "-Isrc",
                    "-include",
                    "src/extra.h",
                    "-gcc-toolchain",
                    "/llvm",
                    "-DNAME=foo.c",
                    "-c",
                    "src/parser.c",
                ]
                .into_iter()
                .map(OsString::from)
                .collect(),
                Path::new("/repo/compat"),
                Path::new("/cargo/tree-sitter-language/wasm/include"),
                Path::new(directory),
            ))
        };
        let swift = arguments("/cargo/swift");
        let cpp = arguments("/cargo/cpp");

        assert!(swift.contains(&"/cargo/swift/src/parser.c".to_owned()));
        assert!(cpp.contains(&"/cargo/cpp/src/parser.c".to_owned()));
        assert!(swift.contains(&"-I/cargo/swift/src".to_owned()));
        assert!(swift.contains(&"/cargo/swift/src/extra.h".to_owned()));
        assert!(swift.contains(&"-gcc-toolchain".to_owned()));
        assert!(swift.contains(&"-DNAME=foo.c".to_owned()));
    }

    #[test]
    fn adapter_snapshots_remain_stable_when_xtask_is_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("xtask");
        let cache = directory.path().join("cache");
        fs::write(&executable, b"original xtask").unwrap();
        let original = stage_compiler_adapter(&executable, &cache, "original").unwrap();

        fs::write(&executable, b"rebuilt xtask").unwrap();
        let reused = stage_compiler_adapter(&executable, &cache, "original").unwrap();
        let updated = stage_compiler_adapter(&executable, &cache, "updated").unwrap();

        assert_eq!(original, reused);
        assert_eq!(fs::read(original).unwrap(), b"original xtask");
        assert_eq!(fs::read(updated).unwrap(), b"rebuilt xtask");
    }

    #[test]
    fn adapter_identity_tracks_header_contents_and_zig_version() {
        let directory = tempfile::tempdir().unwrap();
        let compat = directory.path().join("compat");
        let headers = directory.path().join("headers");
        fs::create_dir(&compat).unwrap();
        fs::create_dir(&headers).unwrap();
        fs::write(compat.join("compat.h"), b"compatibility headers").unwrap();
        fs::write(headers.join("stdint.h"), b"upstream headers").unwrap();
        let original = adapter_fingerprint(&compat, &headers, b"zig 1").unwrap();

        assert_eq!(
            original,
            adapter_fingerprint(&compat, &headers, b"zig 1").unwrap()
        );
        assert_ne!(
            original,
            adapter_fingerprint(&compat, &headers, b"zig 2").unwrap()
        );
        fs::write(compat.join("compat.h"), b"changed compatibility headers").unwrap();
        assert_ne!(
            original,
            adapter_fingerprint(&compat, &headers, b"zig 1").unwrap()
        );
        fs::write(compat.join("compat.h"), b"compatibility headers").unwrap();
        assert_eq!(
            original,
            adapter_fingerprint(&compat, &headers, b"zig 1").unwrap()
        );
        fs::write(headers.join("stdint.h"), b"changed upstream headers").unwrap();
        assert_ne!(
            original,
            adapter_fingerprint(&compat, &headers, b"zig 1").unwrap()
        );
    }
}
