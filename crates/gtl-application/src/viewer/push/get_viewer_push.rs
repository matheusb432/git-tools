use gtl_wire::viewer::push::{ViewerPushRequest, ViewerPushStatus};

use super::{PushError, ViewerPushOperations};

#[cqrsy::query]
pub fn execute(
    request: ViewerPushRequest,
    operations: &ViewerPushOperations,
) -> Result<ViewerPushStatus, PushError> {
    let operations = operations.0.lock().map_err(|_| PushError::State)?;
    operations
        .history
        .iter()
        .find(|operation| operation.id == request.id)
        .map(|operation| operation.status.clone())
        .ok_or(PushError::NotFound)
}
