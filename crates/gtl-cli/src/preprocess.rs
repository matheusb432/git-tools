//! Argv-normalization shim: rewrites legacy top-level `merge-diff`/`squash-preview`
//! invocations onto the grouped `diff merge`/`diff squash` surface before clap ever
//! sees them.
//!
//! * migration bridge: retire once managed wrappers call the grouped form

/// Rewrites a legacy leading `merge-diff` or `squash-preview` token onto `diff
/// merge`/`diff squash`, dropping a `--monorepo <value>` pair from the rewritten
/// tail (the daemon already ignores `monorepo`; the grouped forms dropped the
/// field entirely). Any other argv passes through untouched.
pub fn normalize(argv: Vec<String>) -> Vec<String> {
    match argv.first().map(String::as_str) {
        Some("merge-diff") => rewrite(argv, "merge"),
        Some("squash-preview") => rewrite(argv, "squash"),
        _ => argv,
    }
}

fn rewrite(argv: Vec<String>, sub: &str) -> Vec<String> {
    let mut rest: Vec<String> = argv.into_iter().skip(1).collect();
    strip_monorepo(&mut rest);
    let mut out = vec!["diff".to_string(), sub.to_string()];
    out.append(&mut rest);
    out
}

/// Drops `--monorepo <value>` (or `--monorepo=<value>`) from `args`, wherever it
/// appears.
fn strip_monorepo(args: &mut Vec<String>) {
    let Some(pos) = args
        .iter()
        .position(|arg| arg == "--monorepo" || arg.starts_with("--monorepo="))
    else {
        return;
    };

    if args[pos].starts_with("--monorepo=") {
        args.remove(pos);
    } else if pos + 1 < args.len() {
        args.drain(pos..=pos + 1);
    } else {
        args.remove(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn rewrites_legacy_merge_diff_to_grouped_form() {
        let out = normalize(v(&["merge-diff", "--repo", "r", "--base", "main"]));
        assert_eq!(out, v(&["diff", "merge", "--repo", "r", "--base", "main"]));
    }

    #[test]
    fn rewrites_legacy_squash_preview_to_grouped_form() {
        let out = normalize(v(&["squash-preview", "--repo", "r"]));
        assert_eq!(out, v(&["diff", "squash", "--repo", "r"]));
    }

    #[test]
    fn strips_monorepo_pair_from_legacy_merge_diff() {
        let out = normalize(v(&[
            "merge-diff",
            "--repo",
            "r",
            "--monorepo",
            "m",
            "--base",
            "main",
        ]));
        assert_eq!(out, v(&["diff", "merge", "--repo", "r", "--base", "main"]));
    }

    #[test]
    fn strips_monorepo_pair_from_legacy_squash_preview() {
        let out = normalize(v(&["squash-preview", "--repo", "r", "--monorepo", "m"]));
        assert_eq!(out, v(&["diff", "squash", "--repo", "r"]));
    }

    #[test]
    fn strips_monorepo_equals_form() {
        let out = normalize(v(&["squash-preview", "--repo", "r", "--monorepo=m"]));
        assert_eq!(out, v(&["diff", "squash", "--repo", "r"]));
    }

    #[test]
    fn dangling_monorepo_flag_without_value_is_dropped() {
        let out = normalize(v(&["squash-preview", "--repo", "r", "--monorepo"]));
        assert_eq!(out, v(&["diff", "squash", "--repo", "r"]));
    }

    #[test]
    fn leaves_grouped_and_other_argv_untouched() {
        assert_eq!(
            normalize(v(&["diff", "merge", "--repo", "r"])),
            v(&["diff", "merge", "--repo", "r"])
        );
        assert_eq!(normalize(v(&["diff", "-l", "5"])), v(&["diff", "-l", "5"]));
        assert_eq!(normalize(v(&["status", "--all"])), v(&["status", "--all"]));
        assert_eq!(normalize(Vec::new()), Vec::<String>::new());
    }
}
