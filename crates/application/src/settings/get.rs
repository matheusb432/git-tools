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
#[cqrsy::query]
pub fn execute(
    req: GetSetting,
    store: &impl AppStateStore,
) -> Result<GetSettingResponse, GetSettingError> {
    let GetSetting { data_root, key } = req;
    let value = store.get_setting(&data_root, &key)?;
    Ok(GetSettingResponse { value })
}

#[cfg(test)]
mod tests {
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
        let response = execute(
            GetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
            },
            &store,
        )
        .expect("get succeeds");

        assert_eq!(response.value.as_deref(), Some("hearth"));
    }

    #[test]
    fn missing_key_is_none_not_an_error() {
        let store = InMemoryAppStateStore::default();
        let response = execute(
            GetSetting {
                data_root: "/data".into(),
                key: "absent".into(),
            },
            &store,
        )
        .expect("get succeeds");

        assert_eq!(response.value, None);
    }
}
