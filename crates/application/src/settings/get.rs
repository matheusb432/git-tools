//! The `settings/get` vertical slice: read one app setting by key.

use std::path::PathBuf;

use crate::ports::AppStateStore;

#[derive(Debug, Clone, PartialEq)]
pub struct GetSetting {
    pub data_root: PathBuf,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetSettingResponse {
    pub value: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GetSettingError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Gets a setting through the app-state port.
#[cqrsy::handler(query)]
pub fn handle(
    store: &impl AppStateStore,
    req: GetSetting,
) -> Result<GetSettingResponse, GetSettingError> {
    let value = store.get_setting(&req.data_root, &req.key)?;
    Ok(GetSettingResponse { value })
}

#[cfg(test)]
mod tests {
    use cqrsy::Sender;

    use super::*;
    use crate::testing::InMemoryAppStateStore;

    #[test]
    fn returns_the_stored_value() {
        let store = InMemoryAppStateStore::default();
        store
            .settings
            .lock()
            .unwrap()
            .insert("theme".into(), "hearth".into());
        let handler = GetSettingHandler { store };

        let response = handler
            .send_now(GetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
            })
            .expect("get succeeds");

        assert_eq!(response.value.as_deref(), Some("hearth"));
    }

    #[test]
    fn missing_key_is_none_not_an_error() {
        let handler = GetSettingHandler {
            store: InMemoryAppStateStore::default(),
        };

        let response = handler
            .send_now(GetSetting {
                data_root: "/data".into(),
                key: "absent".into(),
            })
            .expect("get succeeds");

        assert_eq!(response.value, None);
    }
}
