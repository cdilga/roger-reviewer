//! Roger-owned GitHub adapter.
//!
//! Wraps the `gh` CLI for repo/PR target resolution, metadata fetching,
//! anchor validation, and the bounded outbound posting adapter used by
//! Roger's explicit draft -> approve -> post flow.

use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

use roger_app_core::{
    OutboundDraft, OutboundDraftBatch, OutboundPostingAdapter, PostingAdapterItemResult,
    ReviewTarget, now_ts, validate_outbound_draft_batch_linkage,
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
    #[error("unsupported posting target locator: {0}")]
    UnsupportedPostingTarget(String),
    #[error("posting target locator does not match review target: {0}")]
    PostingTargetMismatch(String),
    #[error("anchor validation failed: {reason}")]
    AnchorInvalid { reason: String },
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("failed to normalize gh posting response: {0}")]
    PostingResponseInvalid(String),
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
    runner: Arc<dyn GhCommandRunner>,
}

impl GhCliAdapter {
    pub fn new() -> Self {
        Self {
            gh_path: "gh".to_owned(),
            runner: Arc::new(ProcessGhCommandRunner),
        }
    }

    pub fn with_gh_path(gh_path: String) -> Self {
        Self {
            gh_path,
            runner: Arc::new(ProcessGhCommandRunner),
        }
    }

    #[cfg(test)]
    fn with_runner(gh_path: String, runner: Arc<dyn GhCommandRunner>) -> Self {
        Self { gh_path, runner }
    }

    fn run_gh<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
        self.runner.run(&self.gh_path, &args)
    }
}

impl Default for GhCliAdapter {
    fn default() -> Self {
        Self::new()
    }
}

trait GhCommandRunner: Send + Sync {
    fn run(&self, gh_path: &str, args: &[String]) -> Result<String>;
}

struct ProcessGhCommandRunner;

impl GhCommandRunner for ProcessGhCommandRunner {
    fn run(&self, gh_path: &str, args: &[String]) -> Result<String> {
        let output = Command::new(gh_path).args(args).output().map_err(|e| {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostingFailureClass {
    Retryable,
    Permanent,
}

impl PostingFailureClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Permanent => "permanent",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GitHubPostingTarget {
    IssueComment {
        owner: String,
        repo: String,
        pr: u64,
    },
    ReviewThreadReply {
        owner: String,
        repo: String,
        pr: u64,
        thread_id: String,
    },
}

impl GitHubPostingTarget {
    fn parse(locator: &str) -> Result<Self> {
        if let Some(rest) = locator.strip_prefix("github:issue-comment:") {
            let (repo_slug, pr) = parse_repo_and_pr(rest)?;
            let (owner, repo) = parse_repo_slug(&repo_slug)?;
            return Ok(Self::IssueComment { owner, repo, pr });
        }

        if let Some(rest) = locator.strip_prefix("github:review-thread:") {
            let (repo_and_pr, thread_id) = rest
                .rsplit_once('#')
                .ok_or_else(|| GitHubAdapterError::UnsupportedPostingTarget(locator.to_owned()))?;
            let (repo_slug, pr) = parse_repo_and_pr(repo_and_pr)?;
            let (owner, repo) = parse_repo_slug(&repo_slug)?;
            if thread_id.trim().is_empty() {
                return Err(GitHubAdapterError::UnsupportedPostingTarget(locator.to_owned()));
            }
            return Ok(Self::ReviewThreadReply {
                owner,
                repo,
                pr,
                thread_id: thread_id.to_owned(),
            });
        }

        Err(GitHubAdapterError::UnsupportedPostingTarget(locator.to_owned()))
    }

    fn matches_review_target(&self, target: &ReviewTarget) -> bool {
        self.repo_slug() == target.repository && self.pr_number() == target.pull_request_number
    }

    fn repo_slug(&self) -> String {
        match self {
            Self::IssueComment { owner, repo, .. }
            | Self::ReviewThreadReply { owner, repo, .. } => format!("{owner}/{repo}"),
        }
    }

    fn pr_number(&self) -> u64 {
        match self {
            Self::IssueComment { pr, .. } | Self::ReviewThreadReply { pr, .. } => *pr,
        }
    }
}

fn parse_repo_and_pr(raw: &str) -> Result<(String, u64)> {
    let (repo_slug, pr_raw) = raw
        .rsplit_once('#')
        .ok_or_else(|| GitHubAdapterError::UnsupportedPostingTarget(raw.to_owned()))?;
    let pr = pr_raw
        .parse::<u64>()
        .map_err(|_| GitHubAdapterError::UnsupportedPostingTarget(raw.to_owned()))?;
    Ok((repo_slug.to_owned(), pr))
}

fn parse_repo_slug(raw: &str) -> Result<(String, String)> {
    let (owner, repo) = raw
        .split_once('/')
        .ok_or_else(|| GitHubAdapterError::UnsupportedPostingTarget(raw.to_owned()))?;
    if owner.trim().is_empty() || repo.trim().is_empty() {
        return Err(GitHubAdapterError::UnsupportedPostingTarget(raw.to_owned()));
    }
    Ok((owner.to_owned(), repo.to_owned()))
}

fn normalize_posting_failure(class: PostingFailureClass, code: &str) -> String {
    format!("{}:{code}", class.as_str())
}

fn classify_gh_command_failure(stderr: &str) -> (PostingFailureClass, &'static str) {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("secondary rate limit") || lower.contains("rate limit") {
        (PostingFailureClass::Retryable, "rate_limited")
    } else if lower.contains("timed out") || lower.contains("timeout") {
        (PostingFailureClass::Retryable, "timeout")
    } else if lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("bad gateway")
        || lower.contains("service unavailable")
        || lower.contains("gateway timeout")
    {
        (PostingFailureClass::Retryable, "service_unavailable")
    } else if lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("network")
        || lower.contains("tls")
    {
        (PostingFailureClass::Retryable, "network_error")
    } else if lower.contains("404") || lower.contains("not found") {
        (PostingFailureClass::Permanent, "target_not_found")
    } else if lower.contains("422")
        || lower.contains("unprocessable")
        || lower.contains("validation failed")
    {
        (PostingFailureClass::Permanent, "validation_failed")
    } else if lower.contains("403")
        || lower.contains("forbidden")
        || lower.contains("denied")
        || lower.contains("resource not accessible")
        || lower.contains("permission")
    {
        (PostingFailureClass::Permanent, "write_denied")
    } else {
        (PostingFailureClass::Permanent, "unknown_write_failure")
    }
}

fn failed_posting_result(
    draft: &OutboundDraft,
    class: PostingFailureClass,
    code: &str,
) -> PostingAdapterItemResult {
    PostingAdapterItemResult {
        draft_id: draft.id.clone(),
        status: roger_app_core::PostingAdapterItemStatus::Failed,
        remote_identifier: None,
        failure_code: Some(normalize_posting_failure(class, code)),
    }
}

fn extract_issue_comment_remote_identifier(response: &str) -> Result<String> {
    let raw: serde_json::Value =
        serde_json::from_str(response).map_err(|e| GitHubAdapterError::ParseError(e.to_string()))?;
    if let Some(identifier) = raw
        .get("html_url")
        .and_then(|value| value.as_str())
        .or_else(|| raw.get("url").and_then(|value| value.as_str()))
        .or_else(|| raw.get("node_id").and_then(|value| value.as_str()))
    {
        return Ok(identifier.to_owned());
    }

    if let Some(identifier) = raw.get("id").and_then(|value| value.as_i64()) {
        return Ok(identifier.to_string());
    }

    Err(GitHubAdapterError::PostingResponseInvalid(
        "missing issue comment identifier".to_owned(),
    ))
}

fn extract_thread_reply_remote_identifier(response: &str) -> Result<String> {
    let raw: serde_json::Value =
        serde_json::from_str(response).map_err(|e| GitHubAdapterError::ParseError(e.to_string()))?;
    let comment = raw
        .get("data")
        .and_then(|value| value.get("addPullRequestReviewThreadReply"))
        .and_then(|value| value.get("comment"))
        .ok_or_else(|| {
            GitHubAdapterError::PostingResponseInvalid(
                "missing review thread reply comment payload".to_owned(),
            )
        })?;
    if let Some(identifier) = comment
        .get("url")
        .and_then(|value| value.as_str())
        .or_else(|| comment.get("id").and_then(|value| value.as_str()))
    {
        return Ok(identifier.to_owned());
    }

    if let Some(identifier) = comment.get("databaseId").and_then(|value| value.as_i64()) {
        return Ok(identifier.to_string());
    }

    Err(GitHubAdapterError::PostingResponseInvalid(
        "missing review thread reply identifier".to_owned(),
    ))
}

impl ReadSafeGitHubAdapter for GhCliAdapter {
    fn check_gh_available(&self) -> Result<bool> {
        match self.run_gh(["auth", "status"]) {
            Ok(_) => Ok(true),
            Err(GitHubAdapterError::GhNotFound) => Ok(false),
            Err(GitHubAdapterError::GhCommandFailed { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn resolve_pr(&self, owner: &str, repo: &str, pr_number: u64) -> Result<PullRequestMetadata> {
        let repo_slug = format!("{owner}/{repo}");
        let pr_str = pr_number.to_string();

        let json_str = self.run_gh([
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

        let json_str = self.run_gh([
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
        match self.run_gh(["api", &api_path, "--jq", ".size"]) {
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
        target: &ReviewTarget,
        batch: &OutboundDraftBatch,
        drafts: &[OutboundDraft],
    ) -> std::result::Result<Vec<PostingAdapterItemResult>, String> {
        const REVIEW_THREAD_REPLY_MUTATION: &str = "mutation($threadId: ID!, $body: String!) { addPullRequestReviewThreadReply(input: {pullRequestReviewThreadId: $threadId, body: $body}) { comment { id url databaseId } } }";

        let mut results = Vec::with_capacity(drafts.len());
        for draft in drafts {
            let validation = validate_outbound_draft_batch_linkage(batch, std::slice::from_ref(draft));
            if !validation.valid {
                let code = validation
                    .issues
                    .first()
                    .map(|issue| issue.reason_code.as_str())
                    .unwrap_or("batch_binding_invalid");
                results.push(failed_posting_result(draft, PostingFailureClass::Permanent, code));
                continue;
            }

            let posting_target = match GitHubPostingTarget::parse(&draft.target_locator) {
                Ok(target_locator) => target_locator,
                Err(GitHubAdapterError::UnsupportedPostingTarget(_)) => {
                    results.push(failed_posting_result(
                        draft,
                        PostingFailureClass::Permanent,
                        "unsupported_target_locator",
                    ));
                    continue;
                }
                Err(err) => return Err(err.to_string()),
            };

            if !posting_target.matches_review_target(target) {
                results.push(failed_posting_result(
                    draft,
                    PostingFailureClass::Permanent,
                    "target_mismatch",
                ));
                continue;
            }

            let response = match &posting_target {
                GitHubPostingTarget::IssueComment { owner, repo, pr } => self.run_gh(vec![
                    "api".to_owned(),
                    format!("repos/{owner}/{repo}/issues/{pr}/comments"),
                    "--method".to_owned(),
                    "POST".to_owned(),
                    "-f".to_owned(),
                    format!("body={}", draft.body),
                ]),
                GitHubPostingTarget::ReviewThreadReply {
                    thread_id, ..
                } => self.run_gh(vec![
                    "api".to_owned(),
                    "graphql".to_owned(),
                    "-f".to_owned(),
                    format!("query={REVIEW_THREAD_REPLY_MUTATION}"),
                    "-F".to_owned(),
                    format!("threadId={thread_id}"),
                    "-F".to_owned(),
                    format!("body={}", draft.body),
                ]),
            };

            match response {
                Ok(response) => {
                    let remote_identifier = match posting_target {
                        GitHubPostingTarget::IssueComment { .. } => {
                            extract_issue_comment_remote_identifier(&response)
                        }
                        GitHubPostingTarget::ReviewThreadReply { .. } => {
                            extract_thread_reply_remote_identifier(&response)
                        }
                    };
                    match remote_identifier {
                        Ok(remote_identifier) => results.push(PostingAdapterItemResult {
                            draft_id: draft.id.clone(),
                            status: roger_app_core::PostingAdapterItemStatus::Posted,
                            remote_identifier: Some(remote_identifier),
                            failure_code: None,
                        }),
                        Err(GitHubAdapterError::ParseError(_))
                        | Err(GitHubAdapterError::PostingResponseInvalid(_)) => {
                            results.push(failed_posting_result(
                                draft,
                                PostingFailureClass::Permanent,
                                "ambiguous_remote_result",
                            ));
                        }
                        Err(err) => return Err(err.to_string()),
                    }
                }
                Err(GitHubAdapterError::GhCommandFailed { stderr }) => {
                    let (class, code) = classify_gh_command_failure(&stderr);
                    results.push(failed_posting_result(draft, class, code));
                }
                Err(GitHubAdapterError::GhNotFound) | Err(GitHubAdapterError::IoError(_)) => {
                    return Err(GitHubAdapterError::GhNotFound.to_string());
                }
                Err(err) => return Err(err.to_string()),
            }
        }

        Ok(results)
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
    use std::collections::VecDeque;
    use std::sync::Mutex;

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
    fn issue_comment_remote_identifier_prefers_urls_then_ids() {
        let from_url = extract_issue_comment_remote_identifier(
            r#"{"html_url":"https://github.com/example/test-repo/pull/42#issuecomment-7"}"#,
        )
        .unwrap();
        assert_eq!(
            from_url,
            "https://github.com/example/test-repo/pull/42#issuecomment-7"
        );

        let from_numeric_id = extract_issue_comment_remote_identifier(r#"{"id":17}"#).unwrap();
        assert_eq!(from_numeric_id, "17");
    }

    #[test]
    fn review_thread_remote_identifier_accepts_url_or_database_id() {
        let from_url = extract_thread_reply_remote_identifier(
            r#"{"data":{"addPullRequestReviewThreadReply":{"comment":{"url":"https://github.com/example/test-repo/pull/42#discussion_r12"}}}}"#,
        )
        .unwrap();
        assert_eq!(
            from_url,
            "https://github.com/example/test-repo/pull/42#discussion_r12"
        );

        let from_database_id = extract_thread_reply_remote_identifier(
            r#"{"data":{"addPullRequestReviewThreadReply":{"comment":{"databaseId":91}}}}"#,
        )
        .unwrap();
        assert_eq!(from_database_id, "91");
    }

    fn sample_review_target() -> ReviewTarget {
        ReviewTarget {
            repository: "example/test-repo".to_owned(),
            pull_request_number: 42,
            base_ref: "main".to_owned(),
            head_ref: "feature/outbound".to_owned(),
            base_commit: "aaa111".to_owned(),
            head_commit: "bbb222".to_owned(),
        }
    }

    fn sample_batch() -> OutboundDraftBatch {
        OutboundDraftBatch {
            id: "batch-1".to_owned(),
            review_session_id: "session-1".to_owned(),
            review_run_id: "run-1".to_owned(),
            repo_id: "repo-1".to_owned(),
            remote_review_target_id: "pr-42".to_owned(),
            payload_digest: "sha256:payload-1".to_owned(),
            approval_state: roger_app_core::ApprovalState::Approved,
            approved_at: Some(1_710_000_001),
            invalidated_at: None,
            invalidation_reason_code: None,
            row_version: 1,
        }
    }

    fn sample_draft(id: &str, locator: &str, body: &str) -> OutboundDraft {
        let batch = sample_batch();
        OutboundDraft {
            id: id.to_owned(),
            review_session_id: batch.review_session_id,
            review_run_id: batch.review_run_id,
            finding_id: Some(format!("finding-{id}")),
            draft_batch_id: batch.id,
            repo_id: batch.repo_id,
            remote_review_target_id: batch.remote_review_target_id,
            payload_digest: batch.payload_digest,
            approval_state: roger_app_core::ApprovalState::Approved,
            anchor_digest: format!("anchor:{id}"),
            target_locator: locator.to_owned(),
            body: body.to_owned(),
            row_version: 1,
        }
    }

    struct MockGhRunner {
        expected_calls: Mutex<VecDeque<(Vec<String>, Result<String>)>>,
    }

    impl MockGhRunner {
        fn new(expected_calls: Vec<(Vec<String>, Result<String>)>) -> Self {
            Self {
                expected_calls: Mutex::new(VecDeque::from(expected_calls)),
            }
        }
    }

    impl GhCommandRunner for MockGhRunner {
        fn run(&self, _gh_path: &str, args: &[String]) -> Result<String> {
            let mut expected_calls = self.expected_calls.lock().expect("lock mock gh calls");
            let (expected_args, response) = expected_calls
                .pop_front()
                .expect("unexpected gh invocation");
            assert_eq!(args, expected_args.as_slice());
            response
        }
    }

    #[test]
    fn post_approved_draft_batch_posts_issue_comment_and_thread_reply() {
        let review_thread_mutation = "mutation($threadId: ID!, $body: String!) { addPullRequestReviewThreadReply(input: {pullRequestReviewThreadId: $threadId, body: $body}) { comment { id url databaseId } } }";
        let runner = Arc::new(MockGhRunner::new(vec![
            (
                vec![
                    "api".to_owned(),
                    "repos/example/test-repo/issues/42/comments".to_owned(),
                    "--method".to_owned(),
                    "POST".to_owned(),
                    "-f".to_owned(),
                    "body=Please re-check the issue comment path.".to_owned(),
                ],
                Ok(r#"{"html_url":"https://github.com/example/test-repo/pull/42#issuecomment-7"}"#.to_owned()),
            ),
            (
                vec![
                    "api".to_owned(),
                    "graphql".to_owned(),
                    "-f".to_owned(),
                    format!("query={review_thread_mutation}"),
                    "-F".to_owned(),
                    "threadId=THREAD_node_123".to_owned(),
                    "-F".to_owned(),
                    "body=Please re-check the review thread path.".to_owned(),
                ],
                Ok(r#"{"data":{"addPullRequestReviewThreadReply":{"comment":{"url":"https://github.com/example/test-repo/pull/42#discussion_r12"}}}}"#.to_owned()),
            ),
        ]));
        let adapter = GhCliAdapter::with_runner("gh".to_owned(), runner);
        let batch = sample_batch();
        let drafts = vec![
            sample_draft(
                "draft-issue",
                "github:issue-comment:example/test-repo#42",
                "Please re-check the issue comment path.",
            ),
            sample_draft(
                "draft-thread",
                "github:review-thread:example/test-repo#42#THREAD_node_123",
                "Please re-check the review thread path.",
            ),
        ];

        let results = adapter
            .post_approved_draft_batch(&sample_review_target(), &batch, &drafts)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].remote_identifier.as_deref(),
            Some("https://github.com/example/test-repo/pull/42#issuecomment-7")
        );
        assert_eq!(
            results[1].remote_identifier.as_deref(),
            Some("https://github.com/example/test-repo/pull/42#discussion_r12")
        );
        assert!(results.iter().all(|item| item.failure_code.is_none()));
    }

    #[test]
    fn post_approved_draft_batch_marks_partial_retryable_failure() {
        let runner = Arc::new(MockGhRunner::new(vec![
            (
                vec![
                    "api".to_owned(),
                    "repos/example/test-repo/issues/42/comments".to_owned(),
                    "--method".to_owned(),
                    "POST".to_owned(),
                    "-f".to_owned(),
                    "body=First post succeeds.".to_owned(),
                ],
                Ok(r#"{"id":73}"#.to_owned()),
            ),
            (
                vec![
                    "api".to_owned(),
                    "repos/example/test-repo/issues/42/comments".to_owned(),
                    "--method".to_owned(),
                    "POST".to_owned(),
                    "-f".to_owned(),
                    "body=Second post hits a retryable outage.".to_owned(),
                ],
                Err(GitHubAdapterError::GhCommandFailed {
                    stderr: "HTTP 503 Service Unavailable".to_owned(),
                }),
            ),
        ]));
        let adapter = GhCliAdapter::with_runner("gh".to_owned(), runner);
        let batch = sample_batch();
        let drafts = vec![
            sample_draft(
                "draft-ok",
                "github:issue-comment:example/test-repo#42",
                "First post succeeds.",
            ),
            sample_draft(
                "draft-retry",
                "github:issue-comment:example/test-repo#42",
                "Second post hits a retryable outage.",
            ),
        ];

        let results = adapter
            .post_approved_draft_batch(&sample_review_target(), &batch, &drafts)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].status, roger_app_core::PostingAdapterItemStatus::Posted);
        assert_eq!(results[0].remote_identifier.as_deref(), Some("73"));
        assert_eq!(results[1].status, roger_app_core::PostingAdapterItemStatus::Failed);
        assert_eq!(
            results[1].failure_code.as_deref(),
            Some("retryable:service_unavailable")
        );
    }

    #[test]
    fn post_approved_draft_batch_classifies_permanent_write_failures() {
        let runner = Arc::new(MockGhRunner::new(vec![(
            vec![
                "api".to_owned(),
                "repos/example/test-repo/issues/42/comments".to_owned(),
                "--method".to_owned(),
                "POST".to_owned(),
                "-f".to_owned(),
                "body=GitHub rejects this payload.".to_owned(),
            ],
            Err(GitHubAdapterError::GhCommandFailed {
                stderr: "HTTP 422 Validation Failed".to_owned(),
            }),
        )]));
        let adapter = GhCliAdapter::with_runner("gh".to_owned(), runner);
        let batch = sample_batch();
        let drafts = vec![sample_draft(
            "draft-permanent",
            "github:issue-comment:example/test-repo#42",
            "GitHub rejects this payload.",
        )];

        let results = adapter
            .post_approved_draft_batch(&sample_review_target(), &batch, &drafts)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, roger_app_core::PostingAdapterItemStatus::Failed);
        assert_eq!(
            results[0].failure_code.as_deref(),
            Some("permanent:validation_failed")
        );
    }

    #[test]
    fn post_approved_draft_batch_fails_closed_on_payload_binding_drift() {
        let runner = Arc::new(MockGhRunner::new(Vec::new()));
        let adapter = GhCliAdapter::with_runner("gh".to_owned(), runner);
        let batch = sample_batch();
        let mut draft = sample_draft(
            "draft-drifted",
            "github:issue-comment:example/test-repo#42",
            "Drifted payload should not post.",
        );
        draft.payload_digest = "sha256:payload-drifted".to_owned();

        let results = adapter
            .post_approved_draft_batch(&sample_review_target(), &batch, &[draft])
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, roger_app_core::PostingAdapterItemStatus::Failed);
        assert_eq!(
            results[0].failure_code.as_deref(),
            Some("permanent:payload_digest_mismatch")
        );
    }
}
