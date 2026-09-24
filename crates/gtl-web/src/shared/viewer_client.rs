use dioxus::CapturedError;
pub(crate) use gtl_client::ViewerClientError;

/// Recovers the client error that `use_action` captured from a viewer request.
pub(crate) fn captured_client_error(error: &CapturedError) -> Option<&ViewerClientError> {
    error.downcast_ref()
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::ViewerClientError;

    pub(crate) struct ConnectedViewer {
        pub(crate) client: gtl_client::ViewerClient,
        pub(crate) server_instance_id: String,
    }

    pub(crate) async fn connect_viewer() -> Result<ConnectedViewer, ViewerClientError> {
        let client = gtl_client::ViewerClient::connect().await?;
        let server_instance_id = client.server_instance_id().to_owned();
        Ok(ConnectedViewer {
            client,
            server_instance_id,
        })
    }

    pub(crate) fn discard_viewer_connection() {
        gtl_client::ViewerClient::discard_connection();
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use browser::{connect_viewer, discard_viewer_connection};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn discard_viewer_connection() {}
