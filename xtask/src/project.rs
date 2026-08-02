//! Repository-owned test declarations.

use std::{ffi::OsString, time::Duration};

use sample_project::{Test, summary};

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

pub(crate) fn tests_unit() -> Vec<TestDeclaration> {
    vec![TestDeclaration::new(
        "unit",
        Test::new("unit", "cargo")
            .args(["test", "--quiet"])
            .verbose_arguments(["--", "--nocapture"])
            .summary_parser(summary::cargo),
    )]
}

pub(crate) fn tests_e2e(executable: OsString) -> Vec<TestDeclaration> {
    vec![desktop_e2e(executable)]
}

pub(crate) fn tests_all(executable: OsString) -> Vec<TestDeclaration> {
    vec![
        TestDeclaration::new(
            "unit",
            Test::new("unit", "cargo")
                .args(["test", "--workspace", "--quiet"])
                .verbose_arguments(["--", "--nocapture"])
                .summary_parser(summary::cargo),
        ),
        TestDeclaration::new(
            "web",
            Test::new("web", executable.clone())
                .arg("frontend-test")
                .summary_parser(summary::vitest),
        ),
        worker("drift", &executable, "drift-check"),
        desktop_e2e(executable),
    ]
}

fn worker(label: &'static str, executable: &OsString, verb: &'static str) -> TestDeclaration {
    TestDeclaration::new(label, Test::new(label, executable.clone()).arg(verb))
}

fn desktop_e2e(executable: OsString) -> TestDeclaration {
    TestDeclaration::new(
        "e2e",
        Test::new("e2e", executable)
            .arg("desktop-e2e-worker")
            .verbose_arguments(["--verbose"])
            .accepts_evidences()
            .timeout(Duration::from_hours(2)),
    )
}
