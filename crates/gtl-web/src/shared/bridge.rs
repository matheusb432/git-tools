use dioxus::prelude::document;
use gtl_contracts::viewer::ViewerApiError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const INVOKE_SCRIPT: &str = r#"
const [command, args] = await dioxus.recv();
const invoke = window.__TAURI__?.core?.invoke;
if (typeof invoke !== "function") {
    return { status: "unavailable" };
}
try {
    const value = await invoke(command, args ?? {});
    return { status: "ok", payload: value };
} catch (error) {
    if (error && typeof error === "object" && typeof error.kind === "string") {
        return { status: "api_error", payload: error };
    }
    return { status: "unavailable" };
}
"#;

const EVENT_SCRIPT: &str = r#"
const eventName = await dioxus.recv();
const listen = window.__TAURI__?.event?.listen;
if (typeof listen !== "function") {
    dioxus.send({ status: "unavailable" });
    return;
}
try {
    const unlisten = await listen(eventName, (event) => {
        dioxus.send({ status: "event", payload: event.payload });
    });
    dioxus.send({ status: "ready" });
    try {
        await dioxus.recv();
    } finally {
        unlisten();
    }
} catch (_error) {
    dioxus.send({ status: "unavailable" });
}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientApiError {
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

#[derive(Deserialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case")]
enum Invocation<T> {
    Ok(T),
    ApiError(ViewerApiError),
    Unavailable,
}

#[derive(Deserialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case")]
enum EventMessage<T> {
    Ready,
    Event(T),
    Unavailable,
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
        Self::invoke_with_arguments(command, ()).await
    }

    pub(crate) async fn invoke_request<Request, Response>(
        command: &'static str,
        request: &Request,
    ) -> Result<Response, ClientApiError>
    where
        Request: Serialize,
        Response: DeserializeOwned,
    {
        Self::invoke_with_arguments(command, CommandArguments { request }).await
    }

    async fn invoke_with_arguments<Arguments, Response>(
        command: &'static str,
        arguments: Arguments,
    ) -> Result<Response, ClientApiError>
    where
        Arguments: Serialize,
        Response: DeserializeOwned,
    {
        let evaluator = document::eval(INVOKE_SCRIPT);
        evaluator
            .send((command, arguments))
            .map_err(|_| ClientApiError::Unavailable)?;

        match evaluator
            .join::<Invocation<Response>>()
            .await
            .map_err(|_| ClientApiError::Unavailable)?
        {
            Invocation::Ok(response) => Ok(response),
            Invocation::ApiError(error) => Err(ClientApiError::Backend(error)),
            Invocation::Unavailable => Err(ClientApiError::Unavailable),
        }
    }

    pub(crate) async fn listen_to_event<Event, Ready, Handler>(
        event_name: &'static str,
        mut on_ready: Ready,
        mut on_event: Handler,
    ) -> Result<(), ClientApiError>
    where
        Event: DeserializeOwned,
        Ready: FnMut(),
        Handler: FnMut(Event),
    {
        let mut evaluator = document::eval(EVENT_SCRIPT);
        evaluator
            .send(event_name)
            .map_err(|_| ClientApiError::Unavailable)?;

        loop {
            match evaluator
                .recv::<EventMessage<Event>>()
                .await
                .map_err(|_| ClientApiError::Unavailable)?
            {
                EventMessage::Ready => on_ready(),
                EventMessage::Event(event) => on_event(event),
                EventMessage::Unavailable => return Err(ClientApiError::Unavailable),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::{ViewerApiError, ViewerResource};

    use super::ClientApiError;

    #[test]
    fn backend_details_are_not_exposed_in_user_messages() {
        let unavailable = ClientApiError::Backend(ViewerApiError::Unavailable {
            resource: ViewerResource::DiffDocument,
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
