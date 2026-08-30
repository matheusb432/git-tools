use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::Deserialize;

const CONFIGURATION_FILE_NAME: &str = "dx-book.toml";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Configuration {
    pub(crate) book: BookConfiguration,
    #[serde(default)]
    pub(crate) serve: ServeConfiguration,
    pub(crate) tailwind: TailwindConfiguration,
    #[serde(skip)]
    pub(crate) root: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct BookConfiguration {
    #[serde(default = "default_book_path")]
    pub(crate) path: PathBuf,
    pub(crate) package: String,
    #[serde(default = "default_example")]
    pub(crate) example: String,
    #[serde(default = "default_features")]
    pub(crate) features: Vec<String>,
    #[serde(default)]
    pub(crate) default_features: bool,
    #[serde(default = "default_locked")]
    pub(crate) locked: bool,
    pub(crate) source_directories: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum OpenBrowser {
    #[default]
    No,
    Yes,
}

impl OpenBrowser {
    pub(crate) const fn as_bool(self) -> bool {
        matches!(self, Self::Yes)
    }
}

impl<'de> Deserialize<'de> for OpenBrowser {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(if bool::deserialize(deserializer)? {
            Self::Yes
        } else {
            Self::No
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServeConfiguration {
    #[serde(default = "default_port")]
    pub(crate) port: u16,
    #[serde(default)]
    pub(crate) open: OpenBrowser,
}

impl Default for ServeConfiguration {
    fn default() -> Self {
        Self {
            port: default_port(),
            open: OpenBrowser::No,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TailwindConfiguration {
    pub(crate) input: PathBuf,
    pub(crate) output: PathBuf,
}

impl Configuration {
    pub(crate) fn load() -> Result<Self> {
        Self::load_from(std::env::current_dir().context("resolve the current directory")?)
    }

    fn load_from(root: PathBuf) -> Result<Self> {
        let path = root.join(CONFIGURATION_FILE_NAME);
        let source =
            fs::read_to_string(&path).with_context(|| format!("load {CONFIGURATION_FILE_NAME}"))?;
        let mut configuration = toml::from_str::<Self>(&source)
            .with_context(|| format!("load {CONFIGURATION_FILE_NAME}"))?;
        configuration.root = root;
        Ok(configuration)
    }
}

fn default_book_path() -> PathBuf {
    PathBuf::from(".")
}

fn default_example() -> String {
    "component-preview".to_owned()
}

fn default_features() -> Vec<String> {
    vec!["component-preview".to_owned()]
}

const fn default_locked() -> bool {
    true
}

const fn default_port() -> u16 {
    8080
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_uses_holt_shaped_serve_defaults() {
        let project = tempfile::tempdir().unwrap();
        fs::write(
            project.path().join(CONFIGURATION_FILE_NAME),
            r#"
[book]
package = "example-ui"
source-directories = ["src", "dev"]

[tailwind]
input = "dev/tailwind.css"
output = "assets/storybook.css"
"#,
        )
        .unwrap();

        let configuration = Configuration::load_from(project.path().to_owned()).unwrap();

        assert_eq!(configuration.book.path, PathBuf::from("."));
        assert_eq!(configuration.book.example, "component-preview");
        assert_eq!(configuration.book.features, ["component-preview"]);
        assert!(!configuration.book.default_features);
        assert!(configuration.book.locked);
        assert_eq!(configuration.serve.port, 8080);
        assert_eq!(configuration.serve.open, OpenBrowser::No);
    }

    #[test]
    fn configuration_reads_the_boolean_browser_setting() {
        let project = tempfile::tempdir().unwrap();
        fs::write(
            project.path().join(CONFIGURATION_FILE_NAME),
            r#"
[book]
package = "example-ui"
source-directories = ["src", "dev"]

[serve]
open = true

[tailwind]
input = "dev/tailwind.css"
output = "assets/storybook.css"
"#,
        )
        .unwrap();

        let configuration = Configuration::load_from(project.path().to_owned()).unwrap();

        assert_eq!(configuration.serve.open, OpenBrowser::Yes);
    }
}
