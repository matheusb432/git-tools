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
        Test::try_new("unit", "cargo")
            .expect("unit test definition is valid")
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
            Test::try_new("unit", "cargo")
                .expect("unit test definition is valid")
                .args(["test", "--workspace", "--quiet"])
                .verbose_arguments(["--", "--nocapture"])
                .summary_parser(summary::cargo),
        ),
        TestDeclaration::new(
            "web",
            Test::try_new("web", executable.clone())
                .expect("web test definition is valid")
                .arg("frontend-test")
                .summary_parser(summary::vitest),
        ),
        worker("drift", &executable, "drift-check"),
        desktop_e2e(executable),
    ]
}

fn worker(label: &'static str, executable: &OsString, verb: &'static str) -> TestDeclaration {
    TestDeclaration::new(
        label,
        Test::try_new(label, executable.clone())
            .expect("worker test definition is valid")
            .arg(verb),
    )
}

fn desktop_e2e(executable: OsString) -> TestDeclaration {
    TestDeclaration::new(
        "e2e",
        Test::try_new("e2e", executable)
            .expect("desktop E2E test definition is valid")
            .arg("desktop-e2e-worker")
            .verbose_arguments(["--verbose"])
            .accepts_evidences()
            .timeout(Duration::from_hours(2)),
    )
}
