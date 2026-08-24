pub(crate) use gtl_client::ViewerClientError;

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::cell::RefCell;

    use serde::Deserialize;
    use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

    use super::ViewerClientError;

    const LOAD_VIEWER_CONNECTION_COMMAND: &str = "load_viewer_connection";

    thread_local! {
        static CACHED_CONNECTION: RefCell<Option<ViewerConnection>> = const { RefCell::new(None) };
    }

    #[derive(Clone, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ViewerConnection {
        endpoint: String,
        instance_id: String,
        capability: String,
        protocol_version: u32,
    }

    pub(crate) struct ConnectedViewer {
        pub(crate) client: gtl_client::ViewerClient,
        pub(crate) server_instance_id: String,
    }

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(
            catch,
            js_namespace = ["window", "__TAURI_INTERNALS__"],
            js_name = invoke
        )]
        async fn invoke_tauri(command: &str, arguments: JsValue) -> Result<JsValue, JsValue>;
    }

    pub(crate) async fn connect_viewer() -> Result<ConnectedViewer, ViewerClientError> {
        let connection = load_viewer_connection().await?;
        if connection.instance_id.is_empty() {
            return Err(ViewerClientError::Unavailable);
        }
        let client = gtl_client::ViewerClient::connect_grpc_web(
            connection.endpoint,
            &connection.capability,
            connection.protocol_version,
        )?;
        Ok(ConnectedViewer {
            client,
            server_instance_id: connection.instance_id,
        })
    }

    pub(crate) fn discard_viewer_connection() {
        CACHED_CONNECTION.with(|cached| {
            cached.borrow_mut().take();
        });
    }

    async fn load_viewer_connection() -> Result<ViewerConnection, ViewerClientError> {
        if let Some(connection) = CACHED_CONNECTION.with(|cached| cached.borrow().as_ref().cloned())
        {
            return Ok(connection);
        }
        let value = invoke_tauri(LOAD_VIEWER_CONNECTION_COMMAND, JsValue::UNDEFINED)
            .await
            .map_err(|_| ViewerClientError::Unavailable)?;
        let connection = serde_wasm_bindgen::from_value::<ViewerConnection>(value)
            .map_err(|_| ViewerClientError::Unavailable)?;
        CACHED_CONNECTION.with(|cached| {
            cached.replace(Some(connection.clone()));
        });
        Ok(connection)
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use browser::{connect_viewer, discard_viewer_connection};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn discard_viewer_connection() {}
