//! The `settings/set` vertical slice: upsert one app setting.

use std::path::PathBuf;

use crate::ports::AppStateStore;

#[derive(Debug, Clone, PartialEq)]
pub struct SetSetting {
    pub data_root: PathBuf,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetSettingResponse {}

#[derive(Debug, thiserror::Error)]
pub enum SetSettingError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Sets a value through the app-state port.
#[cqrsy::handler(command)]
pub fn handle(
    store: &impl AppStateStore,
    req: SetSetting,
) -> Result<SetSettingResponse, SetSettingError> {
    store.set_setting(&req.data_root, &req.key, &req.value)?;
    Ok(SetSettingResponse {})
}

#[cfg(test)]
mod tests {
    use cqrsy::Sender;

    use super::*;
    use crate::testing::InMemoryAppStateStore;

    #[test]
    fn stores_and_overwrites_the_value() {
        let store = InMemoryAppStateStore::default();
        let handler = SetSettingHandler {
            store: store.clone(),
        };

        handler
            .send_now(SetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
                value: "dark".into(),
            })
            .expect("set succeeds");
        handler
            .send_now(SetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
                value: "hearth".into(),
            })
            .expect("overwrite succeeds");

        assert_eq!(
            store
                .settings
                .lock()
                .unwrap()
                .get("theme")
                .map(String::as_str),
            Some("hearth")
        );
    }
}
