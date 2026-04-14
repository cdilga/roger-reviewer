//! Roger-owned read-safe GitHub adapter.
//!
//! Wraps the `gh` CLI for repo/PR target resolution, metadata fetching,
//! and anchor validation. No mutation-capable paths are exposed; any
//! write operation fails closed until rr-008.1 lands the posting flow.

use std::collections::HashMap;
use std::process::Command;

use roger_app_core::{
    OutboundDraft, OutboundDraftBatch, OutboundPostingAdapter, PostingAdapterItemResult,
    ReviewTarget, now_ts,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum GitHubAdapterError {
    #[error("gh CLI not found or not executable")]
    GhNotFound,
    #[error("gh CLI returned non-zero exit: {stderr}")]
    GhCommandFailed { stderr: String },
    #[error("failed to parse gh output: {0}")]
    ParseError(String),
    #[error("target not found: {owner}/{repo}#{pr}")]
    TargetNotFound {
        owner: String,
        repo: String,
        pr: u64,
    },
    #[error("anchor validation failed: {reason}")]
    AnchorInvalid { reason: String },
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("mutation not allowed: read-safe adapter does not support write operations")]
    MutationBlocked,
}

pub type Result<T> = std::result::Result<T, GitHubAdapterError>;

/// Resolved PR metadata from GitHub.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestMetadata {
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub state: PullRequestState,
    pub base_ref: String,
    pub head_ref: String,
    pub base_commit: String,
    pub head_commit: String,
    pub author: String,
    pub url: String,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub fetched_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
}

/// A file changed in the PR with its diff status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrChangedFile {
    pub path: String,
    pub status: FileChangeStatus,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unknown,
}

/// Result of validating whether a code anchor (file + line range) still
/// exists at the PR head commit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorValidation {
    pub path: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub valid: bool,
    pub reason: Option<String>,
}

/// Trait for the read-safe GitHub adapter boundary.
///
/// Implementations wrap `gh` CLI or provide test doubles.
/// No mutation methods exist by design — write operations
/// are the responsibility of the posting flow (rr-008.1).
pub trait ReadSafeGitHubAdapter {
    /// Check whether `gh` CLI is available and authenticated.
    fn check_gh_available(&self) -> Result<bool>;

    /// Resolve a PR by owner/repo and number, returning full metadata.
    fn resolve_pr(&self, owner: &str, repo: &str, pr_number: u64) -> Result<PullRequestMetadata>;

    /// Convert resolved PR metadata into a Roger `ReviewTarget`.
    fn to_review_target(&self, meta: &PullRequestMetadata) -> ReviewTarget {
        ReviewTarget {
            repository: format!("{}/{}", meta.owner, meta.repo),
            pull_request_number: meta.number,
            base_ref: meta.base_ref.clone(),
            head_ref: meta.head_ref.clone(),
            base_commit: meta.base_commit.clone(),
            head_commit: meta.head_commit.clone(),
        }
    }

    /// List files changed in a PR.
    fn list_changed_files(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<Vec<PrChangedFile>>;

    /// Validate whether a code anchor (path + optional line range) is
    /// still present at the PR head commit.
    fn validate_anchor(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
    ) -> Result<AnchorValidation>;
}

/// Live implementation backed by the `gh` CLI.
pub struct GhCliAdapter {
    /// Optional override for the gh binary path (for testing).
    gh_path: String,
}

impl GhCliAdapter {
    pub fn new() -> Self {
        Self {
            gh_path: "gh".to_owned(),
        }
    }

    pub fn with_gh_path(gh_path: String) -> Self {
        Self { gh_path }
    }

    fn run_gh(&self, args: &[&str]) -> Result<String> {
        let output = Command::new(&self.gh_path)
            .args(args)
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GitHubAdapterError::GhNotFound
                } else {
                    GitHubAdapterError::IoError(e)
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubAdapterError::GhCommandFailed { stderr });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl Default for GhCliAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadSafeGitHubAdapter for GhCliAdapter {
    fn check_gh_available(&self) -> Result<bool> {
        match self.run_gh(&["auth", "status"]) {
            Ok(_) => Ok(true),
            Err(GitHubAdapterError::GhNotFound) => Ok(false),
            Err(GitHubAdapterError::GhCommandFailed { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn resolve_pr(&self, owner: &str, repo: &str, pr_number: u64) -> Result<PullRequestMetadata> {
        let repo_slug = format!("{owner}/{repo}");
        let pr_str = pr_number.to_string();

        let json_str = self.run_gh(&[
            "pr",
            "view",
            &pr_str,
            "--repo",
            &repo_slug,
            "--json",
            "number,title,state,baseRefName,headRefName,baseRefOid,headRefOid,author,url,additions,deletions,changedFiles",
        ])?;

        let raw: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| GitHubAdapterError::ParseError(e.to_string()))?;

        let state_str = raw["state"].as_str().unwrap_or("UNKNOWN");
        let state = match state_str {
            "OPEN" => PullRequestState::Open,
            "CLOSED" => PullRequestState::Closed,
            "MERGED" => PullRequestState::Merged,
            _ => PullRequestState::Open,
        };

        let author = raw["author"]
            .get("login")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_owned();

        Ok(PullRequestMetadata {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            number: raw["number"].as_u64().unwrap_or(pr_number),
            title: raw["title"].as_str().unwrap_or("").to_owned(),
            state,
            base_ref: raw["baseRefName"].as_str().unwrap_or("").to_owned(),
            head_ref: raw["headRefName"].as_str().unwrap_or("").to_owned(),
            base_commit: raw["baseRefOid"].as_str().unwrap_or("").to_owned(),
            head_commit: raw["headRefOid"].as_str().unwrap_or("").to_owned(),
            author,
            url: raw["url"].as_str().unwrap_or("").to_owned(),
            additions: raw["additions"].as_u64().unwrap_or(0),
            deletions: raw["deletions"].as_u64().unwrap_or(0),
            changed_files: raw["changedFiles"].as_u64().unwrap_or(0),
            fetched_at: now_ts(),
        })
    }

    fn list_changed_files(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<Vec<PrChangedFile>> {
        let repo_slug = format!("{owner}/{repo}");
        let pr_str = pr_number.to_string();

        let json_str = self.run_gh(&[
            "pr", "view", &pr_str, "--repo", &repo_slug, "--json", "files",
        ])?;

        let raw: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| GitHubAdapterError::ParseError(e.to_string()))?;

        let files = raw["files"]
            .as_array()
            .ok_or_else(|| GitHubAdapterError::ParseError("missing files array".to_owned()))?;

        let mut result = Vec::with_capacity(files.len());
        for f in files {
            let path = f["path"].as_str().unwrap_or("").to_owned();
            let status_str = f["status"].as_str().unwrap_or("UNKNOWN");
            let status = match status_str {
                "added" | "ADDED" => FileChangeStatus::Added,
                "modified" | "MODIFIED" | "changed" | "CHANGED" => FileChangeStatus::Modified,
                "removed" | "REMOVED" | "deleted" | "DELETED" => FileChangeStatus::Deleted,
                "renamed" | "RENAMED" => FileChangeStatus::Renamed,
                "copied" | "COPIED" => FileChangeStatus::Copied,
                _ => FileChangeStatus::Unknown,
            };
            result.push(PrChangedFile {
                path,
                status,
                additions: f["additions"].as_u64().unwrap_or(0),
                deletions: f["deletions"].as_u64().unwrap_or(0),
            });
        }
        Ok(result)
    }

    fn validate_anchor(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
    ) -> Result<AnchorValidation> {
        let repo_slug = format!("{owner}/{repo}");

        // Use gh api to check if the file exists at the given commit.
        let api_path = format!("repos/{repo_slug}/contents/{path}?ref={commit}");
        match self.run_gh(&["api", &api_path, "--jq", ".size"]) {
            Ok(size_str) => {
                let file_size: u64 = size_str.trim().parse().unwrap_or(0);
                // If line range was requested, we can only confirm file
                // exists; line-level validation needs content fetch which
                // is bounded by the file size.
                if let (Some(start), Some(end)) = (line_start, line_end) {
                    if file_size == 0 {
                        return Ok(AnchorValidation {
                            path: path.to_owned(),
                            line_start: Some(start),
                            line_end: Some(end),
                            valid: false,
                            reason: Some("file exists but is empty".to_owned()),
                        });
                    }
                }
                Ok(AnchorValidation {
                    path: path.to_owned(),
                    line_start,
                    line_end,
                    valid: true,
                    reason: None,
                })
            }
            Err(GitHubAdapterError::GhCommandFailed { stderr }) => {
                if stderr.contains("404") || stderr.contains("Not Found") {
                    Ok(AnchorValidation {
                        path: path.to_owned(),
                        line_start,
                        line_end,
                        valid: false,
                        reason: Some(format!("file not found at commit {commit}")),
                    })
                } else {
                    Err(GitHubAdapterError::AnchorInvalid { reason: stderr })
                }
            }
            Err(e) => Err(e),
        }
    }
}

impl OutboundPostingAdapter for GhCliAdapter {
    fn post_approved_draft_batch(
        &self,
        _target: &ReviewTarget,
        _batch: &OutboundDraftBatch,
        _drafts: &[OutboundDraft],
    ) -> std::result::Result<Vec<PostingAdapterItemResult>, String> {
        Err(GitHubAdapterError::MutationBlocked.to_string())
    }
}

/// In-memory stub for testing without `gh` CLI.
///
/// Preloaded with PR metadata and file lists; anchor validation
/// checks against the preloaded file set.
#[derive(Clone, Debug, Default)]
pub struct StubGitHubAdapter {
    pub prs: HashMap<(String, String, u64), PullRequestMetadata>,
    pub files: HashMap<(String, String, u64), Vec<PrChangedFile>>,
    pub existing_paths: HashMap<(String, String, String), Vec<String>>,
    pub gh_available: bool,
}

impl StubGitHubAdapter {
    pub fn new() -> Self {
        Self {
            gh_available: true,
            ..Default::default()
        }
    }

    pub fn add_pr(&mut self, meta: PullRequestMetadata) {
        self.prs
            .insert((meta.owner.clone(), meta.repo.clone(), meta.number), meta);
    }

    pub fn add_files(&mut self, owner: &str, repo: &str, pr: u64, files: Vec<PrChangedFile>) {
        self.files
            .insert((owner.to_owned(), repo.to_owned(), pr), files);
    }

    pub fn add_existing_path(&mut self, owner: &str, repo: &str, commit: &str, path: &str) {
        self.existing_paths
            .entry((owner.to_owned(), repo.to_owned(), commit.to_owned()))
            .or_default()
            .push(path.to_owned());
    }
}

impl ReadSafeGitHubAdapter for StubGitHubAdapter {
    fn check_gh_available(&self) -> Result<bool> {
        Ok(self.gh_available)
    }

    fn resolve_pr(&self, owner: &str, repo: &str, pr_number: u64) -> Result<PullRequestMetadata> {
        self.prs
            .get(&(owner.to_owned(), repo.to_owned(), pr_number))
            .cloned()
            .ok_or(GitHubAdapterError::TargetNotFound {
                owner: owner.to_owned(),
                repo: repo.to_owned(),
                pr: pr_number,
            })
    }

    fn list_changed_files(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<Vec<PrChangedFile>> {
        Ok(self
            .files
            .get(&(owner.to_owned(), repo.to_owned(), pr_number))
            .cloned()
            .unwrap_or_default())
    }

    fn validate_anchor(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
    ) -> Result<AnchorValidation> {
        let key = (owner.to_owned(), repo.to_owned(), commit.to_owned());
        let exists = self
            .existing_paths
            .get(&key)
            .is_some_and(|paths| paths.contains(&path.to_owned()));

        Ok(AnchorValidation {
            path: path.to_owned(),
            line_start,
            line_end,
            valid: exists,
            reason: if exists {
                None
            } else {
                Some(format!("file not found at commit {commit}"))
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pr() -> PullRequestMetadata {
        PullRequestMetadata {
            owner: "example".to_owned(),
            repo: "test-repo".to_owned(),
            number: 42,
            title: "Fix widget alignment".to_owned(),
            state: PullRequestState::Open,
            base_ref: "main".to_owned(),
            head_ref: "fix/widget".to_owned(),
            base_commit: "aaa111".to_owned(),
            head_commit: "bbb222".to_owned(),
            author: "dev".to_owned(),
            url: "https://github.com/example/test-repo/pull/42".to_owned(),
            additions: 10,
            deletions: 3,
            changed_files: 2,
            fetched_at: 1000,
        }
    }

    #[test]
    fn stub_resolve_pr_returns_preloaded_metadata() {
        let mut stub = StubGitHubAdapter::new();
        stub.add_pr(sample_pr());

        let meta = stub.resolve_pr("example", "test-repo", 42).unwrap();
        assert_eq!(meta.title, "Fix widget alignment");
        assert_eq!(meta.number, 42);
    }

    #[test]
    fn stub_resolve_pr_not_found() {
        let stub = StubGitHubAdapter::new();
        let err = stub.resolve_pr("example", "test-repo", 99).unwrap_err();
        assert!(matches!(err, GitHubAdapterError::TargetNotFound { .. }));
    }

    #[test]
    fn to_review_target_maps_correctly() {
        let stub = StubGitHubAdapter::new();
        let meta = sample_pr();
        let target = stub.to_review_target(&meta);
        assert_eq!(target.repository, "example/test-repo");
        assert_eq!(target.pull_request_number, 42);
        assert_eq!(target.base_ref, "main");
        assert_eq!(target.head_ref, "fix/widget");
    }

    #[test]
    fn stub_list_changed_files() {
        let mut stub = StubGitHubAdapter::new();
        stub.add_files(
            "example",
            "test-repo",
            42,
            vec![PrChangedFile {
                path: "src/widget.rs".to_owned(),
                status: FileChangeStatus::Modified,
                additions: 10,
                deletions: 3,
            }],
        );

        let files = stub.list_changed_files("example", "test-repo", 42).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/widget.rs");
    }

    #[test]
    fn stub_validate_anchor_found() {
        let mut stub = StubGitHubAdapter::new();
        stub.add_existing_path("example", "test-repo", "bbb222", "src/widget.rs");

        let result = stub
            .validate_anchor(
                "example",
                "test-repo",
                "bbb222",
                "src/widget.rs",
                Some(10),
                Some(20),
            )
            .unwrap();
        assert!(result.valid);
        assert!(result.reason.is_none());
    }

    #[test]
    fn stub_validate_anchor_not_found() {
        let stub = StubGitHubAdapter::new();
        let result = stub
            .validate_anchor("example", "test-repo", "bbb222", "missing.rs", None, None)
            .unwrap();
        assert!(!result.valid);
        assert!(result.reason.as_ref().unwrap().contains("not found"));
    }

    #[test]
    fn stub_gh_unavailable() {
        let mut stub = StubGitHubAdapter::new();
        stub.gh_available = false;
        assert!(!stub.check_gh_available().unwrap());
    }

    #[test]
    fn mutation_blocked_error_message() {
        let err = GitHubAdapterError::MutationBlocked;
        assert!(err.to_string().contains("read-safe"));
    }
}
