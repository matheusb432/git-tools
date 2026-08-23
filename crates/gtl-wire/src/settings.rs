//! Serde values for the human-edited user-settings document.

use serde::{Deserialize, Serialize};

/// A serde value retained until infrastructure validates its TOML meaning.
pub type RawSettingValue = serde_json::Value;

/// The top-level shape of the user-settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserSettingsDocument {
    #[serde(default)]
    pub theme: Option<RawSettingValue>,
    #[serde(default)]
    pub layout: Option<RawSettingValue>,
    #[serde(default)]
    pub density: Option<RawSettingValue>,
    #[serde(default)]
    pub push: Option<PushSettingsDocument>,
    #[serde(default)]
    pub diff: Option<DiffSettingsDocument>,
    #[serde(default)]
    pub projects: Vec<ProjectSettingsDocument>,
}

/// The `[push]` section of the user-settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PushSettingsDocument {
    #[serde(default)]
    pub confirm: Option<RawSettingValue>,
}

/// The `[diff]` section of the user-settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffSettingsDocument {
    #[serde(default)]
    pub exclude: Option<RawSettingValue>,
}

/// One entry in the `[[projects]]` settings array.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettingsDocument {
    #[serde(default)]
    pub name: Option<RawSettingValue>,
    #[serde(default)]
    pub excluded_from_push_all: Option<RawSettingValue>,
    #[serde(default)]
    pub diff: Option<DiffSettingsDocument>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        DiffSettingsDocument, ProjectSettingsDocument, PushSettingsDocument, UserSettingsDocument,
    };

    #[test]
    fn document_serializes_the_settings_sections_and_raw_values() {
        let document = UserSettingsDocument {
            theme: Some(json!("dark")),
            layout: Some(json!("split")),
            density: Some(json!("full")),
            push: Some(PushSettingsDocument {
                confirm: Some(json!(false)),
            }),
            diff: Some(DiffSettingsDocument {
                exclude: Some(json!(["md", "lock"])),
            }),
            projects: vec![ProjectSettingsDocument {
                name: Some(json!("git-tools")),
                excluded_from_push_all: Some(json!(true)),
                diff: Some(DiffSettingsDocument {
                    exclude: Some(json!(["js"])),
                }),
            }],
        };

        assert_eq!(
            serde_json::to_value(document).expect("settings document serializes"),
            json!({
                "theme": "dark",
                "layout": "split",
                "density": "full",
                "push": { "confirm": false },
                "diff": { "exclude": ["md", "lock"] },
                "projects": [{
                    "name": "git-tools",
                    "excluded_from_push_all": true,
                    "diff": { "exclude": ["js"] }
                }]
            })
        );
    }

    #[test]
    fn document_deserialization_preserves_present_invalid_types_and_values() {
        let document: UserSettingsDocument = serde_json::from_value(json!({
            "theme": 7,
            "layout": ["split"],
            "density": "diagonal",
            "push": { "confirm": "yes" },
            "diff": { "exclude": ["md", 3] },
            "projects": [{
                "name": 7,
                "excluded_from_push_all": "yes",
                "diff": { "exclude": ["rs", 3] }
            }]
        }))
        .expect("raw settings document deserializes");

        assert_eq!(document.theme, Some(json!(7)));
        assert_eq!(document.layout, Some(json!(["split"])));
        assert_eq!(document.density, Some(json!("diagonal")));
        assert_eq!(
            document.push.expect("push section is present").confirm,
            Some(json!("yes"))
        );
        assert_eq!(
            document.diff.expect("diff section is present").exclude,
            Some(json!(["md", 3]))
        );
        assert_eq!(document.projects[0].name, Some(json!(7)));
        assert_eq!(
            document.projects[0].excluded_from_push_all,
            Some(json!("yes"))
        );
    }

    #[test]
    fn omitted_sections_and_values_remain_absent() {
        let document: UserSettingsDocument =
            serde_json::from_value(json!({})).expect("empty settings document deserializes");

        assert_eq!(document.theme, None);
        assert_eq!(document.layout, None);
        assert_eq!(document.density, None);
        assert_eq!(document.push, None);
        assert_eq!(document.diff, None);
        assert!(document.projects.is_empty());
    }

    #[test]
    fn unknown_settings_are_rejected_instead_of_silently_ignored() {
        let error = serde_json::from_value::<UserSettingsDocument>(json!({
            "push": { "confrm": false }
        }))
        .expect_err("unknown setting must be rejected");

        assert!(error.to_string().contains("unknown field `confrm`"));
    }

    #[test]
    fn unknown_project_settings_are_rejected_instead_of_silently_ignored() {
        let error = serde_json::from_value::<UserSettingsDocument>(json!({
            "projects": [{ "name": "git-tools", "exclude_from_push_all": true }]
        }))
        .expect_err("unknown project setting must be rejected");

        assert!(
            error
                .to_string()
                .contains("unknown field `exclude_from_push_all`")
        );
    }
}
