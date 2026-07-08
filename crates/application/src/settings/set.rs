//! The `settings/set` vertical slice: upsert one app setting.

use std::path::PathBuf;

use cqrsy::Handler;

use crate::ports::AppStateStore;

#[derive(Debug, Clone, PartialEq, cqrsy::Command)]
#[command(out = SetSettingResponse, err = SetSettingError)]
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

/// Handles [`SetSetting`] by writing through the app-state port.
#[derive(Clone)]
pub struct SetSettingHandler<A: AppStateStore> {
    pub store: A,
}

impl<A: AppStateStore> Handler<SetSetting> for SetSettingHandler<A> {
    async fn handle(&self, req: SetSetting) -> Result<SetSettingResponse, SetSettingError> {
        self.store
            .set_setting(&req.data_root, &req.key, &req.value)?;
        Ok(SetSettingResponse {})
    }
}

#[cfg(test)]
mod tests {
    use cqrsy::send_now;

    use super::*;
    use crate::testing::InMemoryAppStateStore;

    #[test]
    fn stores_and_overwrites_the_value() {
        let store = InMemoryAppStateStore::default();
        let handler = SetSettingHandler {
            store: store.clone(),
        };

        send_now(
            &(),
            &handler,
            SetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
                value: "dark".into(),
            },
        )
        .expect("set succeeds");
        send_now(
            &(),
            &handler,
            SetSetting {
                data_root: "/data".into(),
                key: "theme".into(),
                value: "hearth".into(),
            },
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
