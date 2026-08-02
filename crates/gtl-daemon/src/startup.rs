use std::env::VarError;

use anyhow::{Context as _, anyhow};

const DAEMON_PORT_ENVIRONMENT_VARIABLE: &str = "GIT_TOOLS_DAEMON_PORT";

pub(super) fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init();
}

pub(super) fn daemon_port() -> anyhow::Result<u16> {
    parse_daemon_port(std::env::var(DAEMON_PORT_ENVIRONMENT_VARIABLE))
}

fn parse_daemon_port(raw: Result<String, VarError>) -> anyhow::Result<u16> {
    match raw {
        Ok(raw) => raw.parse().with_context(|| {
            format!("{DAEMON_PORT_ENVIRONMENT_VARIABLE} is not a valid value: {raw:?}")
        }),
        Err(VarError::NotPresent) => Ok(0),
        Err(VarError::NotUnicode(raw)) => Err(anyhow!(
            "{DAEMON_PORT_ENVIRONMENT_VARIABLE} is not valid UTF-8: {}",
            raw.display()
        )),
    }
}

pub(super) async fn shutdown_signal() -> std::io::Result<()> {
    let result = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received (SIGINT)");
    result
}

#[cfg(test)]
mod tests {
    use super::parse_daemon_port;

    #[test]
    fn daemon_port_defaults_only_when_absent() {
        assert_eq!(
            parse_daemon_port(Err(std::env::VarError::NotPresent)).expect("default port"),
            0
        );
        assert_eq!(
            parse_daemon_port(Ok("4317".to_owned())).expect("configured port"),
            4317
        );
        assert!(parse_daemon_port(Ok("not-a-port".to_owned())).is_err());
    }
}
