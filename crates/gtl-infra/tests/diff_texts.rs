use gtl_application::{diffs::store_diff_text, ports::DiffTextReader};
use gtl_infra::app_state::SqliteAppState;
use gtl_models::{diffs::DiffText, failure::DiffTextFailure};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn patch(path: &str) -> Result<DiffText, Box<dyn std::error::Error>> {
    Ok(DiffText::try_new(format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n"
    ))?)
}

#[test]
fn stored_text_survives_reopening_and_invalid_text_is_not_stored() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let text = patch("src/a.rs")?;
    store_diff_text::execute(&text, &mut *database.connection_lock()?)?;
    let invalid = DiffText::try_new("prose without file sections\n".to_owned())?;

    let error = store_diff_text::execute(&invalid, &mut *database.connection_lock()?).unwrap_err();

    assert!(matches!(
        error,
        store_diff_text::StoreDiffTextError::Invalid(DiffTextFailure::NoFiles)
    ));
    assert_eq!(database.diff_text(invalid.id())?, None);
    drop(database);
    assert_eq!(
        SqliteAppState::open(directory.path())?.diff_text(text.id())?,
        Some(text)
    );
    Ok(())
}

#[test]
fn unreferenced_texts_keep_only_the_most_recently_stored() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let texts = (0..17)
        .map(|index| patch(&format!("src/{index}.rs")))
        .collect::<Result<Vec<_>, _>>()?;
    for text in &texts {
        store_diff_text::execute(text, &mut *database.connection_lock()?)?;
    }
    store_diff_text::execute(&texts[1], &mut *database.connection_lock()?)?;
    store_diff_text::execute(&patch("src/17.rs")?, &mut *database.connection_lock()?)?;

    assert_eq!(database.diff_text(texts[0].id())?, None);
    assert_eq!(database.diff_text(texts[2].id())?, None);
    assert_eq!(database.diff_text(texts[1].id())?, Some(texts[1].clone()));
    assert_eq!(database.diff_text(texts[16].id())?, Some(texts[16].clone()));
    Ok(())
}

#[test]
fn texts_open_in_saved_tabs_survive_pruning() -> TestResult {
    use gtl_application::{
        recipes::{Recipe, RecipeSource, TextRecipeSource},
        viewer::saved_tabs::{self, SavedViewerTab},
    };

    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let opened = patch("src/opened.rs")?;
    store_diff_text::execute(&opened, &mut *database.connection_lock()?)?;
    let label = gtl_models::paths::ProjectName::try_new("opened.diff".to_owned())?;
    saved_tabs::save(
        &mut *database.connection_lock()?,
        &[SavedViewerTab {
            history_id: None,
            comparison_name: None,
            label: gtl_models::recipes::RecipeLabel::Repository {
                repository: label.clone(),
            },
            recipe: Recipe {
                source: RecipeSource::Text(TextRecipeSource {
                    id: opened.id().clone(),
                    label,
                }),
                name: None,
            },
            pinned: false,
            live: false,
            active: true,
        }],
    )?;

    for index in 0..17 {
        store_diff_text::execute(
            &patch(&format!("src/{index}.rs"))?,
            &mut *database.connection_lock()?,
        )?;
    }

    assert_eq!(database.diff_text(opened.id())?, Some(opened));
    Ok(())
}
