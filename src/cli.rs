#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SquashPreview {
        repo: String,
        monorepo: String,
    },
    Diff {
        repo: String,
        monorepo: String,
        base: Option<String>,
    },
    MergeDiff {
        repo: String,
        monorepo: String,
        base: Option<String>,
    },
    SquashLocal {
        repo: String,
        message: String,
        dry: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError {
    message: String,
}

impl UsageError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn parse(args: &[String]) -> Result<Command, UsageError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(UsageError::new("missing command"));
    };

    match command.as_str() {
        "squash-preview" => {
            let flags = parse_repo_monorepo(rest)?;
            Ok(Command::SquashPreview {
                repo: flags.repo,
                monorepo: flags.monorepo,
            })
        }
        "diff" => {
            let flags = parse_repo_monorepo_base(rest)?;
            Ok(Command::Diff {
                repo: flags.repo,
                monorepo: flags.monorepo,
                base: flags.base,
            })
        }
        "merge-diff" => {
            let flags = parse_repo_monorepo_base(rest)?;
            Ok(Command::MergeDiff {
                repo: flags.repo,
                monorepo: flags.monorepo,
                base: flags.base,
            })
        }
        "squash-local" => parse_squash_local(rest),
        other => Err(UsageError::new(format!("unknown command '{other}'"))),
    }
}

struct RepoMonorepo {
    repo: String,
    monorepo: String,
}

struct RepoMonorepoBase {
    repo: String,
    monorepo: String,
    base: Option<String>,
}

fn parse_repo_monorepo(args: &[String]) -> Result<RepoMonorepo, UsageError> {
    let parsed = parse_repo_monorepo_base(args)?;
    if parsed.base.is_some() {
        return Err(UsageError::new("--base is not valid for squash-preview"));
    }
    Ok(RepoMonorepo {
        repo: parsed.repo,
        monorepo: parsed.monorepo,
    })
}

fn parse_repo_monorepo_base(args: &[String]) -> Result<RepoMonorepoBase, UsageError> {
    let mut repo = None;
    let mut monorepo = None;
    let mut base = None;
    let mut cursor = args.iter();

    while let Some(arg) = cursor.next() {
        if flag_eq(arg, "--repo") {
            repo = Some(next_value(&mut cursor, "--repo")?);
        } else if flag_eq(arg, "--monorepo") {
            monorepo = Some(next_value(&mut cursor, "--monorepo")?);
        } else if flag_eq(arg, "--base") {
            base = Some(next_value(&mut cursor, "--base")?);
        } else {
            return Err(UsageError::new(format!("unknown argument '{arg}'")));
        }
    }

    Ok(RepoMonorepoBase {
        repo: required(repo, "--repo")?,
        monorepo: required(monorepo, "--monorepo")?,
        base,
    })
}

fn parse_squash_local(args: &[String]) -> Result<Command, UsageError> {
    let mut repo = None;
    let mut message = None;
    let mut dry = false;
    let mut cursor = args.iter();

    while let Some(arg) = cursor.next() {
        if flag_eq(arg, "--repo") {
            repo = Some(next_value(&mut cursor, "--repo")?);
        } else if arg == "--" {
            if message.is_some() {
                return Err(UsageError::new("unexpected argument '--'"));
            }
            message = Some(next_value(&mut cursor, "--")?);
        } else if is_dry_alias(arg) {
            dry = true;
        } else if arg.starts_with('-') {
            return Err(UsageError::new(format!("unknown argument '{arg}'")));
        } else if message.is_none() {
            message = Some(arg.clone());
        } else {
            return Err(UsageError::new(format!("unexpected argument '{arg}'")));
        }
    }

    Ok(Command::SquashLocal {
        repo: required(repo, "--repo")?,
        message: required(message, "message")?,
        dry,
    })
}

fn flag_eq(arg: &str, flag: &str) -> bool {
    arg.eq_ignore_ascii_case(flag)
}

fn is_dry_alias(arg: &str) -> bool {
    ["--dry", "-dry", "--dry-run", "-dryrun"]
        .iter()
        .any(|alias| arg.eq_ignore_ascii_case(alias))
}

fn next_value<'a>(
    cursor: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<String, UsageError> {
    let Some(value) = cursor.next() else {
        return Err(UsageError::new(format!("{flag} requires a value")));
    };
    Ok(value.clone())
}

fn required(value: Option<String>, name: &str) -> Result<String, UsageError> {
    value.ok_or_else(|| UsageError::new(format!("{name} is required")))
}

#[cfg(test)]
mod tests {
    use super::{Command, parse};

    fn args(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn parses_squash_preview_repo_and_monorepo_flags() {
        assert_eq!(
            parse(&args(&["squash-preview", "--repo", "r", "--monorepo", "m"])),
            Ok(Command::SquashPreview {
                repo: "r".to_string(),
                monorepo: "m".to_string()
            })
        );
    }

    #[test]
    fn parses_diff_without_base() {
        assert_eq!(
            parse(&args(&["diff", "--repo", "r", "--monorepo", "m"])),
            Ok(Command::Diff {
                repo: "r".to_string(),
                monorepo: "m".to_string(),
                base: None
            })
        );
    }

    #[test]
    fn parses_diff_with_base() {
        assert_eq!(
            parse(&args(&[
                "diff",
                "--repo",
                "r",
                "--monorepo",
                "m",
                "--base",
                "feature"
            ])),
            Ok(Command::Diff {
                repo: "r".to_string(),
                monorepo: "m".to_string(),
                base: Some("feature".to_string())
            })
        );
    }

    #[test]
    fn accepts_flag_looking_values_after_flags() {
        assert_eq!(
            parse(&args(&[
                "diff",
                "--repo",
                "-repo",
                "--monorepo",
                "m",
                "--base",
                "-branch"
            ])),
            Ok(Command::Diff {
                repo: "-repo".to_string(),
                monorepo: "m".to_string(),
                base: Some("-branch".to_string())
            })
        );
    }

    #[test]
    fn parses_merge_diff_with_explicit_base() {
        assert_eq!(
            parse(&args(&[
                "merge-diff",
                "--repo",
                "r",
                "--monorepo",
                "m",
                "--base",
                "feature"
            ])),
            Ok(Command::MergeDiff {
                repo: "r".to_string(),
                monorepo: "m".to_string(),
                base: Some("feature".to_string())
            })
        );
    }

    #[test]
    fn parses_merge_diff_without_base_leaves_default_to_command_layer() {
        assert_eq!(
            parse(&args(&["merge-diff", "--repo", "r", "--monorepo", "m"])),
            Ok(Command::MergeDiff {
                repo: "r".to_string(),
                monorepo: "m".to_string(),
                base: None
            })
        );
    }

    #[test]
    fn parses_squash_local_positional_message_and_dry_aliases() {
        for dry_alias in ["--dry", "-dry", "--dry-run", "-DryRun", "-Dry"] {
            assert_eq!(
                parse(&args(&[
                    "squash-local",
                    "collapse commits",
                    "--repo",
                    "r",
                    dry_alias
                ])),
                Ok(Command::SquashLocal {
                    repo: "r".to_string(),
                    message: "collapse commits".to_string(),
                    dry: true
                }),
                "alias {dry_alias} should enable dry-run"
            );
        }
    }

    #[test]
    fn parses_squash_local_dash_prefixed_message_after_options_marker() {
        assert_eq!(
            parse(&args(&[
                "squash-local",
                "--",
                "- fix commit",
                "--repo",
                "r"
            ])),
            Ok(Command::SquashLocal {
                repo: "r".to_string(),
                message: "- fix commit".to_string(),
                dry: false
            })
        );
    }

    #[test]
    fn missing_value_at_end_of_flag_errors() {
        assert!(parse(&args(&["diff", "--repo"])).is_err());
    }

    #[test]
    fn unknown_command_is_usage_error() {
        assert!(parse(&args(&["bogus"])).is_err());
    }
}
