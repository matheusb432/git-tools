use gtl_wire::viewer::{FieldUpdate, file_filters::UpdateDiffExclusions};

use super::{ViewerState, file_filters::FileFiltersError};
use crate::{
    ports::UserSettingsEditor,
    settings::{DiffExclusionsUpdate, UserSettingsFieldUpdate, UserSettingsPatch, edit_settings},
};

#[cqrsy::command]
pub fn execute(
    request: UpdateDiffExclusions,
    settings: &mut impl UserSettingsEditor,
    state: &ViewerState,
) -> Result<(), FileFiltersError> {
    let extensions = match request.extensions {
        FieldUpdate::Update(value) => UserSettingsFieldUpdate::Update(value),
        FieldUpdate::Clear => UserSettingsFieldUpdate::Clear,
        FieldUpdate::Unchanged => return Ok(()),
    };
    edit_settings::execute(
        UserSettingsPatch {
            diff_exclusions: Some(DiffExclusionsUpdate {
                project: request.project,
                extensions,
                expected: request.expected,
            }),
            ..UserSettingsPatch::default()
        },
        settings,
        state,
    )?;
    Ok(())
}
