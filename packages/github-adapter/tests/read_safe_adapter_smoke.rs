//! Smoke tests for the read-safe GitHub adapter using the stub backend.
//! These tests validate the adapter trait contract without requiring
//! `gh` CLI or network access.

use roger_github_adapter::{
    FileChangeStatus, GitHubAdapterError, PrChangedFile, PullRequestMetadata, PullRequestState,
    ReadSafeGitHubAdapter, StubGitHubAdapter,
};

fn sample_pr() -> PullRequestMetadata {
    PullRequestMetadata {
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        number: 7,
        title: "Add frobnicate method".to_owned(),
        state: PullRequestState::Open,
        base_ref: "main".to_owned(),
        head_ref: "feat/frob".to_owned(),
        base_commit: "abc123".to_owned(),
        head_commit: "def456".to_owned(),
        author: "engineer".to_owned(),
        url: "https://github.com/acme/widgets/pull/7".to_owned(),
        additions: 50,
        deletions: 5,
        changed_files: 3,
        fetched_at: 1000,
    }
}

#[test]
fn resolve_pr_and_convert_to_review_target() {
    let mut adapter = StubGitHubAdapter::new();
    adapter.add_pr(sample_pr());

    let meta = adapter.resolve_pr("acme", "widgets", 7).unwrap();
    assert_eq!(meta.title, "Add frobnicate method");

    let target = adapter.to_review_target(&meta);
    assert_eq!(target.repository, "acme/widgets");
    assert_eq!(target.pull_request_number, 7);
    assert_eq!(target.base_ref, "main");
    assert_eq!(target.head_commit, "def456");
}

#[test]
fn resolve_pr_target_not_found_returns_error() {
    let adapter = StubGitHubAdapter::new();
    let result = adapter.resolve_pr("acme", "widgets", 999);
    assert!(result.is_err());
    match result.unwrap_err() {
        GitHubAdapterError::TargetNotFound { owner, repo, pr } => {
            assert_eq!(owner, "acme");
            assert_eq!(repo, "widgets");
            assert_eq!(pr, 999);
        }
        other => panic!("expected TargetNotFound, got: {other}"),
    }
}

#[test]
fn list_changed_files_returns_preloaded_set() {
    let mut adapter = StubGitHubAdapter::new();
    adapter.add_files(
        "acme",
        "widgets",
        7,
        vec![
            PrChangedFile {
                path: "src/frob.rs".to_owned(),
                status: FileChangeStatus::Added,
                additions: 45,
                deletions: 0,
            },
            PrChangedFile {
                path: "src/lib.rs".to_owned(),
                status: FileChangeStatus::Modified,
                additions: 5,
                deletions: 5,
            },
        ],
    );

    let files = adapter.list_changed_files("acme", "widgets", 7).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "src/frob.rs");
    assert!(matches!(files[0].status, FileChangeStatus::Added));
}

#[test]
fn list_changed_files_empty_for_unknown_pr() {
    let adapter = StubGitHubAdapter::new();
    let files = adapter.list_changed_files("acme", "widgets", 999).unwrap();
    assert!(files.is_empty());
}

#[test]
fn validate_anchor_existing_file() {
    let mut adapter = StubGitHubAdapter::new();
    adapter.add_existing_path("acme", "widgets", "def456", "src/frob.rs");

    let result = adapter
        .validate_anchor(
            "acme",
            "widgets",
            "def456",
            "src/frob.rs",
            Some(1),
            Some(10),
        )
        .unwrap();
    assert!(result.valid);
    assert_eq!(result.path, "src/frob.rs");
    assert_eq!(result.line_start, Some(1));
    assert_eq!(result.line_end, Some(10));
}

#[test]
fn validate_anchor_missing_file() {
    let adapter = StubGitHubAdapter::new();
    let result = adapter
        .validate_anchor("acme", "widgets", "def456", "gone.rs", None, None)
        .unwrap();
    assert!(!result.valid);
    assert!(result.reason.unwrap().contains("not found"));
}

#[test]
fn check_gh_available_stub() {
    let mut adapter = StubGitHubAdapter::new();
    assert!(adapter.check_gh_available().unwrap());

    adapter.gh_available = false;
    assert!(!adapter.check_gh_available().unwrap());
}

#[test]
fn mutation_blocked_error_is_explicit() {
    let err = GitHubAdapterError::MutationBlocked;
    let msg = err.to_string();
    assert!(msg.contains("read-safe"));
    assert!(msg.contains("write"));
}
