use gtl_models::{
    diffs::{CommitIdAbbreviation, DiffViewTitle, ExtensionFilterMode},
    settings::ViewerLanguage,
};

/// Copy owned by the offline document, independent of the viewer catalog.
pub(super) struct Labels {
    pub files: &'static str,
    pub commits: &'static str,
    pub no_commits: &'static str,
    pub repositories: &'static str,
    pub empty: &'static str,
    pub expand_line: &'static str,
    pub hidden_files: &'static str,
    pub old_line: &'static str,
    pub new_line: &'static str,
    pub line: &'static str,
    pub source: &'static str,
    language: ViewerLanguage,
}

impl Labels {
    pub const fn new(language: ViewerLanguage) -> Self {
        match language {
            ViewerLanguage::EnUs => Self {
                files: "Changed files",
                commits: "Commits",
                no_commits: "No commits",
                repositories: "Repositories",
                empty: "No changes",
                expand_line: "Show complete line",
                hidden_files: "Files hidden by the saved extension filter",
                old_line: "Old line",
                new_line: "New line",
                line: "Line",
                source: "Source",
                language,
            },
            ViewerLanguage::PtBr => Self {
                files: "Arquivos alterados",
                commits: "Commits",
                no_commits: "Nenhum commit",
                repositories: "Repositórios",
                empty: "Nenhuma alteração",
                expand_line: "Mostrar linha completa",
                hidden_files: "Arquivos ocultos pelo filtro de extensões salvo",
                old_line: "Linha anterior",
                new_line: "Linha nova",
                line: "Linha",
                source: "Código",
                language,
            },
        }
    }

    pub fn title(title: &DiffViewTitle) -> String {
        match title {
            DiffViewTitle::Diff => "diff".into(),
            DiffViewTitle::MergeDiff => "merge-diff".into(),
            DiffViewTitle::Commit { id } => format!(
                "commit {}",
                id.abbreviated(CommitIdAbbreviation::TenCharacters)
            ),
            DiffViewTitle::Named { name } => name.to_string(),
        }
    }

    pub fn commit_count(&self, count: usize) -> String {
        match (self.language, count) {
            (ViewerLanguage::PtBr, 0) => "nenhum commit".into(),
            (_, 1) => "1 commit".into(),
            (_, _) => format!("{count} commits"),
        }
    }

    pub const fn filter_mode(&self, mode: ExtensionFilterMode) -> &'static str {
        match (self.language, mode) {
            (ViewerLanguage::EnUs, ExtensionFilterMode::Hide) => "hide",
            (ViewerLanguage::EnUs, ExtensionFilterMode::Only) => "show only",
            (ViewerLanguage::PtBr, ExtensionFilterMode::Hide) => "ocultar",
            (ViewerLanguage::PtBr, ExtensionFilterMode::Only) => "mostrar apenas",
        }
    }
}
