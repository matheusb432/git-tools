//! Repository-owned test declarations.

use std::{ffi::OsString, time::Duration};

use anyhow::Result;
use sample_project::{Test, surface};

pub(crate) struct TestDeclaration {
    #[cfg(test)]
    label: &'static str,
    test: Test,
}

impl TestDeclaration {
    fn new(label: &'static str, test: Test) -> Self {
        #[cfg(not(test))]
        let _ = label;
        Self {
            #[cfg(test)]
            label,
            test,
        }
    }

    #[cfg(test)]
    pub(crate) fn label(&self) -> &'static str {
        self.label
    }

    pub(crate) fn into_test(self) -> Test {
        self.test
    }
}

pub(crate) fn tests_unit() -> Result<Vec<TestDeclaration>> {
    Ok(vec![TestDeclaration::new(
        "unit",
        Test::try_new("unit", surface::CARGO, "cargo")?
            .args(["test", "--quiet"])
            .verbose_arguments(["--", "--nocapture"]),
    )])
}

pub(crate) fn tests_e2e(executable: OsString) -> Result<Vec<TestDeclaration>> {
    Ok(vec![cli_e2e()?, desktop_e2e(executable)?])
}

pub(crate) fn tests_all(executable: OsString) -> Result<Vec<TestDeclaration>> {
    Ok(vec![
        TestDeclaration::new(
            "unit",
            Test::try_new("unit", surface::CARGO, "cargo")?
                .args(["test", "--workspace", "--quiet"])
                .verbose_arguments(["--", "--nocapture"]),
        ),
        TestDeclaration::new(
            "web",
            Test::try_new("web", surface::VITEST, executable.clone())?.arg("frontend-test"),
        ),
        worker("drift", &executable, "drift-check")?,
        cli_e2e()?,
        desktop_e2e(executable)?,
    ])
}

fn worker(
    label: &'static str,
    executable: &OsString,
    verb: &'static str,
) -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        label,
        Test::try_new(label, surface::OPAQUE, executable.clone())?.arg(verb),
    ))
}

fn desktop_e2e(executable: OsString) -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "desktop-e2e",
        Test::try_new("desktop-e2e", surface::OPAQUE, executable)?
            .arg("desktop-e2e-worker")
            .verbose_arguments(["--verbose"])
            .accepts_evidences()
            .timeout(Duration::from_hours(2)),
    ))
}

fn cli_e2e() -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "cli-e2e",
        Test::try_new("cli-e2e", surface::CARGO, "cargo")?
            .args([
                "test",
                "-p",
                "gtl-cli-e2e",
                "--features",
                "e2e",
                "--test",
                "daemon_lifecycle",
                "--",
                "--test-threads",
                "1",
            ])
            .verbose_arguments(["--nocapture"])
            .timeout(Duration::from_mins(5)),
    ))
}
