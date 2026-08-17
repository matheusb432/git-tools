//! Typed discovery and health records shared by daemon process roots.

use std::num::{NonZeroU16, NonZeroU32};

use nutype::nutype;
use serde::{Deserialize, Serialize};

/// Identifies a running daemon process and excludes the OS-reserved zero value.
#[nutype(
    const_fn,
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        From,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct DaemonProcessId(NonZeroU32);

/// Identifies a concrete TCP port on the daemon's loopback listener.
#[nutype(
    const_fn,
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        From,
        FromStr,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct DaemonLoopbackPort(NonZeroU16);

/// Exact executable length reported by filesystem metadata, in bytes.
#[nutype(
    const_fn,
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)
)]
pub struct ExecutableByteLength(u64);

/// Executable modification time in milliseconds since the Unix epoch.
#[nutype(
    const_fn,
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)
)]
pub struct ExecutableModifiedUnixMillis(u64);

/// Filesystem identity used to reject a running daemon from another executable revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExeIdentity {
    #[serde(rename = "exe_len")]
    pub length: ExecutableByteLength,
    #[serde(rename = "exe_modified_ms")]
    pub modified_unix_millis: ExecutableModifiedUnixMillis,
}

impl ExeIdentity {
    pub const fn new(
        length: ExecutableByteLength,
        modified_unix_millis: ExecutableModifiedUnixMillis,
    ) -> Self {
        Self {
            length,
            modified_unix_millis,
        }
    }
}

/// Discovery record persisted as `daemon.json` under the daemon store root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortFile {
    pub port: DaemonLoopbackPort,
    pub pid: DaemonProcessId,
}

/// Startup identity returned by `GET /health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    pub pid: DaemonProcessId,
    pub version: String,
    #[serde(flatten)]
    pub exe_identity: ExeIdentity,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn process_id(value: u32) -> DaemonProcessId {
        DaemonProcessId::new(value.try_into().expect("positive fixture process ID"))
    }

    fn port(value: u16) -> DaemonLoopbackPort {
        DaemonLoopbackPort::new(value.try_into().expect("positive fixture loopback port"))
    }

    #[test]
    fn port_file_preserves_the_discovery_json_shape() {
        let port_file = PortFile {
            port: port(4321),
            pid: process_id(99),
        };
        let json = json!({"port": 4321, "pid": 99});

        assert_eq!(serde_json::to_value(port_file).unwrap(), json);
        assert_eq!(serde_json::from_value::<PortFile>(json).unwrap(), port_file);
    }

    #[test]
    fn health_preserves_the_flat_identity_json_shape() {
        let health = Health {
            pid: process_id(42),
            version: "1.2.3".to_owned(),
            exe_identity: ExeIdentity::new(
                ExecutableByteLength::new(8192),
                ExecutableModifiedUnixMillis::new(1_723_456_789),
            ),
        };
        let json = json!({
            "pid": 42,
            "version": "1.2.3",
            "exe_len": 8192,
            "exe_modified_ms": 1_723_456_789
        });

        assert_eq!(serde_json::to_value(&health).unwrap(), json);
        assert_eq!(serde_json::from_value::<Health>(json).unwrap(), health);
    }

    #[test]
    fn operational_records_reject_zero_process_ids_and_ports() {
        assert!(serde_json::from_value::<PortFile>(json!({"port": 0, "pid": 99})).is_err());
        assert!(serde_json::from_value::<PortFile>(json!({"port": 4321, "pid": 0})).is_err());
        assert!(
            serde_json::from_value::<Health>(json!({
                "pid": 0,
                "version": "1.2.3",
                "exe_len": 8192,
                "exe_modified_ms": 1_723_456_789
            }))
            .is_err()
        );
    }

    #[test]
    fn loopback_port_parsing_rejects_zero_and_out_of_range_values() {
        assert_eq!("4321".parse::<DaemonLoopbackPort>().unwrap(), port(4321));
        assert!("0".parse::<DaemonLoopbackPort>().is_err());
        assert!("65536".parse::<DaemonLoopbackPort>().is_err());
        assert!("not-a-port".parse::<DaemonLoopbackPort>().is_err());
    }
}
