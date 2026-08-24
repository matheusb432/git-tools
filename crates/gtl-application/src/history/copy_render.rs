//! Formats one persisted render as the explicit JSON copied by the viewer.

use serde::Serialize;

use crate::{history::RecentRenderRecord, recipes::RecipeOp};

#[derive(Serialize)]
struct HistoryCopy<'record> {
    id: gtl_models::viewer::RenderHistoryId,
    title: &'record str,
    repo_name: &'record gtl_models::paths::ProjectName,
    kind: HistoryCopyKind,
    range_label: &'record str,
    rendered_at: &'record gtl_models::timestamps::MachineTimestamp,
    recipe: &'record crate::recipes::Recipe,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum HistoryCopyKind {
    Diff,
    MergeDiff,
}

/// Formats the stable, complete history record copied by the viewer.
pub fn format(record: &RecentRenderRecord) -> Result<String, serde_json::Error> {
    let kind = match &record.recipe.op {
        RecipeOp::Diff { .. } => HistoryCopyKind::Diff,
        RecipeOp::MergeDiff { .. } => HistoryCopyKind::MergeDiff,
    };
    serde_json::to_string_pretty(&HistoryCopy {
        id: record.id,
        title: &record.title,
        repo_name: &record.repo_name,
        kind,
        range_label: &record.range_label,
        rendered_at: &record.rendered_at,
        recipe: &record.recipe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history::RecentRenderRecord,
        recipes::{Recipe, RecipeOp, RecipeSource},
    };

    #[test]
    fn copied_json_keeps_the_established_explicit_shape() {
        let record = RecentRenderRecord {
            id: gtl_models::viewer::RenderHistoryId::try_new(31).expect("positive render ID"),
            title: "Release diff".to_owned(),
            repo_name: crate::utils::project_name("git-tools"),
            range_label: "main...release".to_owned(),
            rendered_at: "2026-08-09T10:00:00Z"
                .try_into()
                .expect("valid fixture timestamp"),
            recipe: Recipe {
                source: RecipeSource::LocalRepo(crate::utils::repository_root("/repos/git-tools")),
                op: RecipeOp::MergeDiff {
                    base: Some(crate::utils::git_revision("main")),
                    pinned: None,
                },
                name: Some(crate::utils::project_name("release")),
            },
        };

        let copied = format(&record).expect("history copy formats");
        let json: serde_json::Value = serde_json::from_str(&copied).expect("copy is JSON");

        assert_eq!(json["id"], 31);
        assert_eq!(json["repo_name"], "git-tools");
        assert_eq!(json["kind"], "merge-diff");
        assert_eq!(json["recipe"]["source"]["value"], "/repos/git-tools");
        assert_eq!(json["recipe"]["op"]["op"], "merge_diff");
    }
}
