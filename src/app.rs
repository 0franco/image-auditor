use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Instant,
};

use anyhow::Result;
use ratatui::widgets::{ListState, ScrollbarState, TableState};

use crate::llm::{LlmClient, LlmSuggestion, SuggestedPatch};
use crate::scanner::{Issue, IssueKind, IssueSeverity, ScanResult};

// ─── Screen ──────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
pub enum Screen {
    Menu,
    Scanning,
    Results,
    Detail,
}

// ─── App ─────────────────────────────────────────────────────────────────────

pub struct App {
    pub screen: Screen,

    // Menu
    pub menu_state: ListState,
    pub input_path: String,
    pub input_mode: bool,

    // Scanning
    pub scan_start: Option<Instant>,
    pub scan_result: Option<ScanResult>,
    pub scan_error: Option<String>,
    pub scan_rx: Option<mpsc::Receiver<Result<ScanResult>>>,

    // Results
    pub table_state: TableState,
    pub scroll_state: ScrollbarState,
    pub active_tab: usize,

    // Filters
    pub filter_severity: Option<IssueSeverity>,
    pub search_query: String,
    pub search_mode: bool,
    pub cached_filtered_indices: Vec<usize>,

    // Detail
    pub detail_issue: Option<Issue>,
    pub detail_scroll: u16,

    // LLM / patch
    pub detail_suggestion: Option<String>,
    pub detail_suggested_patch: Option<SuggestedPatch>,
    pub detail_loading_suggestion: bool,
    pub detail_suggestion_error: Option<String>,
    pub detail_suggestion_rx: Option<mpsc::Receiver<Result<LlmSuggestion>>>,
    pub detail_patch_confirm_mode: bool,
    pub detail_llm_confirm_mode: bool,
    pub detail_patch_error: Option<String>,
    pub patch_success: Option<PatchSuccess>,

    // Notifications
    pub save_success_time: Option<Instant>,
    pub copy_success_time: Option<Instant>,

    // Infrastructure
    pub running: bool,
    pub llm_client: Option<Arc<dyn LlmClient>>,
    pub tick: u64,
}

/// Holds info about the last successfully applied patch for the success overlay.
pub struct PatchSuccess {
    pub file: PathBuf,
    pub line: usize,
    pub at: Instant,
}

impl App {
    pub fn new() -> Self {
        let mut menu_state = ListState::default();
        menu_state.select(Some(0));
        let mut table_state = TableState::default();
        table_state.select(Some(0));

        Self {
            screen: Screen::Menu,
            menu_state,
            input_path: ".".into(),
            input_mode: false,
            scan_start: None,
            scan_result: None,
            scan_error: None,
            scan_rx: None,
            table_state,
            scroll_state: ScrollbarState::default(),
            active_tab: 0,
            filter_severity: None,
            search_query: String::new(),
            search_mode: false,
            cached_filtered_indices: Vec::new(),
            detail_issue: None,
            detail_scroll: 0,
            detail_suggestion: None,
            detail_suggested_patch: None,
            detail_loading_suggestion: false,
            detail_suggestion_error: None,
            detail_suggestion_rx: None,
            detail_patch_confirm_mode: false,
            detail_llm_confirm_mode: false,
            detail_patch_error: None,
            patch_success: None,
            save_success_time: None,
            copy_success_time: None,
            running: true,
            llm_client: None,
            tick: 0,
        }
    }

    /// Rebuild the filtered issue index cache from current filter/search state.
    pub fn update_filter_cache(&mut self) {
        let Some(ref result) = self.scan_result else {
            self.cached_filtered_indices.clear();
            return;
        };

        self.cached_filtered_indices = result
            .issues
            .iter()
            .enumerate()
            .filter(|(_, i)| self.matches_search(i))
            .filter(|(_, i)| self.matches_severity(i))
            .filter(|(_, i)| self.matches_tab(i))
            .map(|(idx, _)| idx)
            .collect();
    }

    pub fn filtered_issues(&self) -> Vec<Issue> {
        let Some(ref result) = self.scan_result else {
            return vec![];
        };
        self.cached_filtered_indices
            .iter()
            .filter_map(|&idx| result.issues.get(idx).cloned())
            .collect()
    }

    // ── Filter predicates ─────────────────────────────────────────────────

    fn matches_search(&self, issue: &Issue) -> bool {
        if self.search_query.is_empty() {
            return true;
        }
        let q = self.search_query.to_lowercase();
        let name = issue
            .file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        name.contains(&q)
    }

    fn matches_severity(&self, issue: &Issue) -> bool {
        match &self.filter_severity {
            None => true,
            Some(f) => &issue.severity == f,
        }
    }

    fn matches_tab(&self, issue: &Issue) -> bool {
        match self.active_tab {
            0 => true,
            1 => matches!(issue.kind, IssueKind::WrongFormat),
            2 => matches!(issue.kind, IssueKind::MissingWidthHeight),
            3 => matches!(issue.kind, IssueKind::MissingLazyLoading),
            4 => matches!(issue.kind, IssueKind::OversizedFile),
            5 => matches!(issue.kind, IssueKind::MissingSrcset),
            _ => true,
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::types::{Issue, IssueKind, IssueSeverity, ScanResult};
    use std::path::PathBuf;

    fn make_issue(kind: IssueKind, severity: IssueSeverity, file: &str) -> Issue {
        Issue {
            kind,
            severity,
            file: PathBuf::from(file),
            line: 1,
            snippet: String::new(),
            message: String::new(),
        }
    }

    fn app_with_issues(issues: Vec<Issue>) -> App {
        let mut app = App::new();
        app.scan_result = Some(ScanResult { issues, files_scanned: 1, images_found: 1 });
        app.update_filter_cache();
        app
    }

    #[test]
    fn filter_by_severity_error() {
        let mut app = app_with_issues(vec![
            make_issue(IssueKind::MissingWidthHeight, IssueSeverity::Error, "a.html"),
            make_issue(IssueKind::WrongFormat, IssueSeverity::Warning, "b.html"),
        ]);
        app.filter_severity = Some(IssueSeverity::Error);
        app.update_filter_cache();
        let issues = app.filtered_issues();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, IssueSeverity::Error);
    }

    #[test]
    fn filter_by_tab() {
        let mut app = app_with_issues(vec![
            make_issue(IssueKind::WrongFormat, IssueSeverity::Warning, "a.html"),
            make_issue(IssueKind::MissingWidthHeight, IssueSeverity::Error, "b.html"),
        ]);
        app.active_tab = 1; // Format tab
        app.update_filter_cache();
        let issues = app.filtered_issues();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::WrongFormat);
    }

    #[test]
    fn search_by_filename() {
        let mut app = app_with_issues(vec![
            make_issue(IssueKind::WrongFormat, IssueSeverity::Warning, "hero.html"),
            make_issue(IssueKind::WrongFormat, IssueSeverity::Warning, "footer.html"),
        ]);
        app.search_query = "hero".to_string();
        app.update_filter_cache();
        let issues = app.filtered_issues();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].file.to_string_lossy().contains("hero"));
    }

    #[test]
    fn no_filter_returns_all() {
        let mut app = app_with_issues(vec![
            make_issue(IssueKind::WrongFormat, IssueSeverity::Warning, "a.html"),
            make_issue(IssueKind::MissingWidthHeight, IssueSeverity::Error, "b.html"),
            make_issue(IssueKind::MissingSrcset, IssueSeverity::Info, "c.html"),
        ]);
        app.update_filter_cache();
        assert_eq!(app.filtered_issues().len(), 3);
    }
}
