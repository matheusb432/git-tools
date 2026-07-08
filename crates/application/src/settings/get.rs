//! The `settings/get` vertical slice: read one app setting by key.

use std::path::PathBuf;

use cqrsy::Handler;

use crate::ports::AppStateStore;

#[derive(Debug, Clone, PartialEq, cqrsy::Query)]
#[query(out = GetSettingResponse, err = GetSettingError)]
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

/// Handles [`GetSetting`] by reading through the app-state port.
#[derive(Clone)]
pub struct GetSettingHandler<A: AppStateStore> {
    pub store: A,
}

impl<A: AppStateStore> Handler<GetSetting> for GetSettingHandler<A> {
    async fn handle(&self, req: GetSetting) -> Result<GetSettingResponse, GetSettingError> {
        let value = self.store.get_setting(&req.data_root, &req.key)?;
        Ok(GetSettingResponse { value })
    }
}

#[cfg(test)]
mod tests {
    use cqrsy::send_now;

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

        let response = send_now(
            &(),
            &handler,
            GetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
            },
        )
        .expect("get succeeds");

        assert_eq!(response.value.as_deref(), Some("hearth"));
    }

    #[test]
    fn missing_key_is_none_not_an_error() {
        let handler = GetSettingHandler {
            store: InMemoryAppStateStore::default(),
        };

        let response = send_now(
            &(),
            &handler,
            GetSetting {
                data_root: "/data".into(),
                key: "absent".into(),
            },
        )
        .expect("get succeeds");

        assert_eq!(response.value, None);
    }
}
