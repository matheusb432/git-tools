use std::{path::Path, time::Duration};

use anyhow::{Context as _, Result, bail};

pub struct ServerHarness {
    _data_root: tempfile::TempDir,
}

impl ServerHarness {
    pub fn start(settings_path: Option<&Path>) -> Result<Self> {
        let data_root = tempfile::tempdir().context("create gtl-server data root")?;
        // Each integration-test file is a separate process with one server test, so these
        // process-wide variables are established before either the server thread or CLI child.
        unsafe {
            std::env::set_var("GIT_TOOLS_DATA_DIR", data_root.path());
            if let Some(settings_path) = settings_path {
                std::env::set_var("GIT_TOOLS_CONFIG", settings_path);
            } else {
                std::env::remove_var("GIT_TOOLS_CONFIG");
            }
        }
        let (failure_tx, failure_rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("build gtl-server test runtime")
                .and_then(|runtime| {
                    runtime
                        .block_on(gtl_server::run())
                        .context("run gtl-server for CLI integration test")
                });
            if let Err(error) = result {
                drop(failure_tx.send(error.to_string()));
            }
        });

        let endpoint = data_root.path().join("server").join("endpoint.json");
        for _ in 0..100 {
            match failure_rx.try_recv() {
                Ok(error) => bail!(error),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    bail!("gtl-server test thread exited before publishing its endpoint");
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if endpoint.is_file() {
                return Ok(Self {
                    _data_root: data_root,
                });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        bail!(
            "gtl-server did not publish its endpoint at {}",
            endpoint.display()
        )
    }
}
