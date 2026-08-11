use gtl_contracts::viewer::ViewerApiError;
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientApiError {
    #[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
    Backend(ViewerApiError),
    Unavailable,
}

impl ClientApiError {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::Backend(ViewerApiError::InvalidRequest) => {
                "The viewer rejected this request. Refresh the page and try again."
            }
            Self::Backend(ViewerApiError::NotFound { .. }) => {
                "This viewer item is no longer available."
            }
            Self::Backend(ViewerApiError::Conflict) => {
                "The viewer changed while this action was running. Try again."
            }
            Self::Backend(ViewerApiError::Unavailable { .. }) | Self::Unavailable => {
                "The desktop viewer is temporarily unavailable."
            }
            Self::Backend(ViewerApiError::Internal) => "The viewer could not complete this action.",
        }
    }
}

#[derive(Serialize)]
struct CommandArguments<'request, Request> {
    request: &'request Request,
}

pub(crate) struct TauriBridge;

impl TauriBridge {
    pub(crate) async fn invoke<Response>(command: &'static str) -> Result<Response, ClientApiError>
    where
        Response: DeserializeOwned,
    {
        bindings::invoke(command, &()).await
    }

    pub(crate) async fn invoke_request<Request, Response>(
        command: &'static str,
        request: &Request,
    ) -> Result<Response, ClientApiError>
    where
        Request: Serialize,
        Response: DeserializeOwned,
    {
        bindings::invoke(command, &CommandArguments { request }).await
    }

    pub(crate) async fn listen_to_event<Event, Ready, Handler>(
        event_name: &'static str,
        on_ready: Ready,
        on_event: Handler,
    ) -> Result<(), ClientApiError>
    where
        Event: DeserializeOwned + 'static,
        Ready: FnMut() + 'static,
        Handler: FnMut(Event) + 'static,
    {
        bindings::listen(event_name, on_ready, on_event).await
    }
}

#[cfg(target_arch = "wasm32")]
mod bindings {
    use std::future;

    use js_sys::{Function, Promise};
    use serde::Deserialize;
    use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
    use wasm_bindgen_futures::JsFuture;

    use super::{ClientApiError, DeserializeOwned, Serialize, ViewerApiError};

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(
            js_namespace = ["window", "__TAURI__", "core"],
            js_name = invoke,
            catch
        )]
        fn tauri_invoke(command: &str, arguments: &JsValue) -> Result<Promise, JsValue>;

        #[wasm_bindgen(
            js_namespace = ["window", "__TAURI__", "event"],
            js_name = listen,
            catch
        )]
        fn tauri_listen(event_name: &str, handler: &Function) -> Result<Promise, JsValue>;
    }

    #[derive(Deserialize)]
    struct TauriEvent<Event> {
        payload: Event,
    }

    struct EventSubscription {
        _callback: Closure<dyn FnMut(JsValue)>,
        unlisten: Function,
    }

    impl Drop for EventSubscription {
        fn drop(&mut self) {
            let _ = self.unlisten.call0(&JsValue::UNDEFINED);
        }
    }

    pub(super) async fn invoke<Arguments, Response>(
        command: &'static str,
        arguments: &Arguments,
    ) -> Result<Response, ClientApiError>
    where
        Arguments: Serialize,
        Response: DeserializeOwned,
    {
        let arguments =
            serde_wasm_bindgen::to_value(arguments).map_err(|_| ClientApiError::Unavailable)?;
        let promise = tauri_invoke(command, &arguments).map_err(invocation_error)?;
        let response = JsFuture::from(promise).await.map_err(invocation_error)?;
        serde_wasm_bindgen::from_value(response).map_err(|_| ClientApiError::Unavailable)
    }

    pub(super) async fn listen<Event, Ready, Handler>(
        event_name: &'static str,
        mut on_ready: Ready,
        mut on_event: Handler,
    ) -> Result<(), ClientApiError>
    where
        Event: DeserializeOwned + 'static,
        Ready: FnMut() + 'static,
        Handler: FnMut(Event) + 'static,
    {
        let callback = Closure::new(move |value: JsValue| {
            if let Ok(event) = serde_wasm_bindgen::from_value::<TauriEvent<Event>>(value) {
                on_event(event.payload);
            }
        });
        let promise = tauri_listen(event_name, callback.as_ref().unchecked_ref())
            .map_err(|_| ClientApiError::Unavailable)?;
        let unlisten = JsFuture::from(promise)
            .await
            .map_err(|_| ClientApiError::Unavailable)?
            .dyn_into::<Function>()
            .map_err(|_| ClientApiError::Unavailable)?;
        let _subscription = EventSubscription {
            _callback: callback,
            unlisten,
        };
        on_ready();
        future::pending::<()>().await;
        Ok(())
    }

    fn invocation_error(value: JsValue) -> ClientApiError {
        serde_wasm_bindgen::from_value::<ViewerApiError>(value)
            .map(ClientApiError::Backend)
            .unwrap_or(ClientApiError::Unavailable)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod bindings {
    use super::{ClientApiError, DeserializeOwned, Serialize};

    pub(super) async fn invoke<Arguments, Response>(
        _command: &'static str,
        _arguments: &Arguments,
    ) -> Result<Response, ClientApiError>
    where
        Arguments: Serialize,
        Response: DeserializeOwned,
    {
        Err(ClientApiError::Unavailable)
    }

    pub(super) async fn listen<Event, Ready, Handler>(
        _event_name: &'static str,
        _on_ready: Ready,
        _on_event: Handler,
    ) -> Result<(), ClientApiError>
    where
        Event: DeserializeOwned + 'static,
        Ready: FnMut() + 'static,
        Handler: FnMut(Event) + 'static,
    {
        Err(ClientApiError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::{ViewerApiError, ViewerResource};

    use super::ClientApiError;

    #[test]
    fn backend_details_are_not_exposed_in_user_messages() {
        let unavailable = ClientApiError::Backend(ViewerApiError::Unavailable {
            resource: ViewerResource::DiffLines,
        });
        let missing = ClientApiError::Backend(ViewerApiError::NotFound {
            resource: ViewerResource::HistoryEntry,
        });

        assert_eq!(
            unavailable.message(),
            "The desktop viewer is temporarily unavailable."
        );
        assert_eq!(
            missing.message(),
            "This viewer item is no longer available."
        );
    }
}
