use std::{env, net::SocketAddr};

use anyhow::{Context as _, bail};

const BIND_ADDRESS_ENVIRONMENT_VARIABLE: &str = "GTL_SERVER_BIND_ADDRESS";
const DEFAULT_BIND_ADDRESS: SocketAddr = SocketAddr::V4(std::net::SocketAddrV4::new(
    std::net::Ipv4Addr::LOCALHOST,
    0,
));

#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) bind_address: SocketAddr,
}

impl Config {
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let bind_address = match env::var(BIND_ADDRESS_ENVIRONMENT_VARIABLE) {
            Ok(value) => Some(value),
            Err(env::VarError::NotPresent) => None,
            Err(env::VarError::NotUnicode(_)) => {
                bail!("{BIND_ADDRESS_ENVIRONMENT_VARIABLE} must contain Unicode")
            }
        };

        Self::from_bind_address(bind_address.as_deref())
    }

    fn from_bind_address(bind_address: Option<&str>) -> anyhow::Result<Self> {
        let bind_address = bind_address.map_or(Ok(DEFAULT_BIND_ADDRESS), |value| {
            value.parse::<SocketAddr>().with_context(|| {
                format!("{BIND_ADDRESS_ENVIRONMENT_VARIABLE} must be a socket address")
            })
        })?;
        if !bind_address.ip().is_loopback() {
            bail!("{BIND_ADDRESS_ENVIRONMENT_VARIABLE} must use a loopback address");
        }

        Ok(Self { bind_address })
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::{Config, DEFAULT_BIND_ADDRESS};

    #[test]
    fn defaults_to_the_loopback_service_address() {
        let config = Config::from_bind_address(None).expect("default config is valid");

        assert_eq!(config.bind_address, DEFAULT_BIND_ADDRESS);
    }

    #[test]
    fn accepts_ipv4_and_ipv6_loopback_addresses() {
        for address in ["127.0.0.1:6000", "[::1]:6000"] {
            let config =
                Config::from_bind_address(Some(address)).expect("loopback config is valid");

            assert_eq!(config.bind_address, address.parse::<SocketAddr>().unwrap());
        }
    }

    #[test]
    fn rejects_non_loopback_and_malformed_addresses() {
        for address in ["0.0.0.0:50051", "192.0.2.1:50051", "localhost:50051"] {
            let error = Config::from_bind_address(Some(address))
                .expect_err("invalid bind address must fail");

            assert!(error.to_string().contains("GTL_SERVER_BIND_ADDRESS"));
        }
    }
}
