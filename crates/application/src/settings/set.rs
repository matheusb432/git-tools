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
pub fn execute(
    req: SetSetting,
    store: &impl AppStateStore,
) -> Result<SetSettingResponse, SetSettingError> {
    let SetSetting {
        data_root,
        key,
        value,
    } = req;
    store.set_setting(&data_root, &key, &value)?;
    Ok(SetSettingResponse {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::InMemoryAppStateStore;

    #[test]
    fn stores_and_overwrites_the_value() {
        let store = InMemoryAppStateStore::default();
        execute(
            SetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
                value: "dark".into(),
            },
            &store,
        )
        .expect("set succeeds");
        execute(
            SetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
                value: "hearth".into(),
            },
            &store,
        )
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
