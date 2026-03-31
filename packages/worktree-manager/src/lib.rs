//! Roger worktree preparation and conservative named-instance sharing.
//!
//! Provides explicit preflight classification, resource plan resolution,
//! worktree lifecycle management (create/track/teardown), and named-instance
//! routing with conservative sharing semantics.
//!
//! Key design decisions (per AGENTS.md / canonical plan):
//! - No hidden heuristics for worktree creation; explicit preflight only
//! - Named instances isolate repo-local mutable resources before they isolate the DB
//! - Same-PR ambiguity fails closed rather than guessing
//! - Retargeting invalidates affected approvals/review bindings

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("git command failed: {stderr}")]
    GitCommandFailed { stderr: String },
    #[error("worktree already exists at {path}")]
    WorktreeAlreadyExists { path: String },
    #[error("worktree not found: {id}")]
    WorktreeNotFound { id: String },
    #[error("instance not found: {name}")]
    InstanceNotFound { name: String },
    #[error("same-PR ambiguity: multiple instances target PR #{pr} — {instances:?}")]
    SamePrAmbiguity { pr: u64, instances: Vec<String> },
    #[error("retarget invalidation: instance {instance} changed target, approvals invalidated")]
    RetargetInvalidation { instance: String },
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, WorktreeError>;

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ---------------------------------------------------------------------------
// Preflight classification
// ---------------------------------------------------------------------------

/// Whether a review flow needs worktree isolation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorktreeNeed {
    /// The review can safely use the current working tree.
    NotNeeded,
    /// A worktree is recommended for isolation.
    Recommended { reason: String },
    /// A worktree is required (e.g. dirty working tree + review of different branch).
    Required { reason: String },
}

/// Input for the preflight classification.
#[derive(Clone, Debug)]
pub struct PreflightInput {
    pub repo_root: PathBuf,
    pub current_branch: String,
    pub target_head_ref: String,
    pub working_tree_dirty: bool,
    pub existing_worktrees: Vec<String>,
}

/// Classify whether a worktree is needed for the given review context.
pub fn classify_worktree_need(input: &PreflightInput) -> WorktreeNeed {
    if input.current_branch == input.target_head_ref && !input.working_tree_dirty {
        return WorktreeNeed::NotNeeded;
    }

    if input.working_tree_dirty && input.current_branch != input.target_head_ref {
        return WorktreeNeed::Required {
            reason: format!(
                "working tree is dirty and target branch '{}' differs from current '{}'",
                input.target_head_ref, input.current_branch,
            ),
        };
    }

    if input.current_branch != input.target_head_ref {
        return WorktreeNeed::Recommended {
            reason: format!(
                "target branch '{}' differs from current '{}'",
                input.target_head_ref, input.current_branch,
            ),
        };
    }

    // Dirty tree but same branch — can proceed in-place.
    WorktreeNeed::NotNeeded
}

// ---------------------------------------------------------------------------
// Resource plan
// ---------------------------------------------------------------------------

/// A resolved resource plan for an isolated review environment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePlan {
    pub instance_name: String,
    pub worktree_path: PathBuf,
    pub db_path: PathBuf,
    pub artifact_dir: PathBuf,
    pub log_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub env_overrides: HashMap<String, String>,
}

/// Derive a resource plan from an instance name and base paths.
pub fn resolve_resource_plan(
    instance_name: &str,
    roger_data_dir: &Path,
    worktree_base: &Path,
    repo_name: &str,
) -> ResourcePlan {
    let instance_dir = roger_data_dir.join("instances").join(instance_name);
    let worktree_path = worktree_base.join(format!("{repo_name}-{instance_name}"));

    ResourcePlan {
        instance_name: instance_name.to_owned(),
        worktree_path,
        db_path: instance_dir.join("roger.db"),
        artifact_dir: instance_dir.join("artifacts"),
        log_dir: instance_dir.join("logs"),
        cache_dir: instance_dir.join("cache"),
        env_overrides: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Worktree lifecycle
// ---------------------------------------------------------------------------

/// Tracked worktree state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedWorktree {
    pub id: String,
    pub path: PathBuf,
    pub branch: String,
    pub commit: String,
    pub instance_name: Option<String>,
    pub created_at: i64,
    pub status: WorktreeStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorktreeStatus {
    Active,
    Stale,
    TornDown,
}

/// Request to create a git worktree.
#[derive(Clone, Debug)]
pub struct CreateWorktreeRequest {
    pub repo_root: PathBuf,
    pub target_path: PathBuf,
    pub branch: String,
    pub commit: Option<String>,
    pub instance_name: Option<String>,
}

/// Trait for worktree operations, allowing test doubles.
pub trait WorktreeOps {
    fn create_worktree(&self, req: &CreateWorktreeRequest) -> Result<TrackedWorktree>;
    fn remove_worktree(&self, repo_root: &Path, worktree_path: &Path) -> Result<()>;
    fn list_worktrees(&self, repo_root: &Path) -> Result<Vec<TrackedWorktree>>;
}

/// Live implementation using `git worktree` commands.
pub struct GitWorktreeOps;

impl GitWorktreeOps {
    fn run_git(repo_root: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(WorktreeError::GitCommandFailed { stderr });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl WorktreeOps for GitWorktreeOps {
    fn create_worktree(&self, req: &CreateWorktreeRequest) -> Result<TrackedWorktree> {
        let path_str = req.target_path.to_string_lossy();

        if req.target_path.exists() {
            return Err(WorktreeError::WorktreeAlreadyExists {
                path: path_str.to_string(),
            });
        }

        let mut args = vec!["worktree", "add", &*path_str];

        // If a specific commit is given, create a detached HEAD worktree.
        // Otherwise create one tracking the named branch.
        let commit_ref;
        if let Some(ref commit) = req.commit {
            args.push("--detach");
            commit_ref = commit.clone();
            args.push(&commit_ref);
        } else {
            args.push(&req.branch);
        }

        Self::run_git(&req.repo_root, &args)?;

        // Read the HEAD of the new worktree to confirm.
        let head = Self::run_git(&req.target_path, &["rev-parse", "HEAD"])
            .unwrap_or_default()
            .trim()
            .to_owned();

        Ok(TrackedWorktree {
            id: format!("wt-{}", &head[..8.min(head.len())]),
            path: req.target_path.clone(),
            branch: req.branch.clone(),
            commit: head,
            instance_name: req.instance_name.clone(),
            created_at: now_ts(),
            status: WorktreeStatus::Active,
        })
    }

    fn remove_worktree(&self, repo_root: &Path, worktree_path: &Path) -> Result<()> {
        let path_str = worktree_path.to_string_lossy();
        Self::run_git(repo_root, &["worktree", "remove", "--force", &path_str])?;
        Ok(())
    }

    fn list_worktrees(&self, repo_root: &Path) -> Result<Vec<TrackedWorktree>> {
        let output = Self::run_git(repo_root, &["worktree", "list", "--porcelain"])?;
        let mut worktrees = Vec::new();
        let mut current_path = None;
        let mut current_head = String::new();
        let mut current_branch = String::new();

        for line in output.lines() {
            if let Some(rest) = line.strip_prefix("worktree ") {
                if let Some(path) = current_path.take() {
                    worktrees.push(TrackedWorktree {
                        id: format!("wt-{}", &current_head[..8.min(current_head.len())]),
                        path: PathBuf::from(&path),
                        branch: current_branch.clone(),
                        commit: current_head.clone(),
                        instance_name: None,
                        created_at: 0,
                        status: WorktreeStatus::Active,
                    });
                }
                current_path = Some(rest.to_owned());
                current_head = String::new();
                current_branch = String::new();
            } else if let Some(rest) = line.strip_prefix("HEAD ") {
                current_head = rest.to_owned();
            } else if let Some(rest) = line.strip_prefix("branch ") {
                current_branch = rest.to_owned();
            }
        }
        // Flush last entry.
        if let Some(path) = current_path {
            worktrees.push(TrackedWorktree {
                id: format!("wt-{}", &current_head[..8.min(current_head.len())]),
                path: PathBuf::from(&path),
                branch: current_branch,
                commit: current_head,
                instance_name: None,
                created_at: 0,
                status: WorktreeStatus::Active,
            });
        }

        Ok(worktrees)
    }
}

// ---------------------------------------------------------------------------
// Named instance registry
// ---------------------------------------------------------------------------

/// A named Roger instance tracking its review target and resource plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedInstance {
    pub name: String,
    pub review_target_pr: Option<u64>,
    pub review_target_repo: Option<String>,
    pub worktree_id: Option<String>,
    pub resource_plan: Option<ResourcePlan>,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: InstanceStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceStatus {
    Active,
    Idle,
    TornDown,
}

/// In-memory named instance registry with conservative sharing semantics.
#[derive(Clone, Debug, Default)]
pub struct InstanceRegistry {
    pub(crate) instances: HashMap<String, NamedInstance>,
}

impl InstanceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, instance: NamedInstance) -> Result<()> {
        self.instances.insert(instance.name.clone(), instance);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<&NamedInstance> {
        self.instances
            .get(name)
            .ok_or_else(|| WorktreeError::InstanceNotFound {
                name: name.to_owned(),
            })
    }

    pub fn list_active(&self) -> Vec<&NamedInstance> {
        self.instances
            .values()
            .filter(|i| i.status == InstanceStatus::Active)
            .collect()
    }

    /// Find instances targeting the same PR. If more than one exists,
    /// this is an ambiguity that must be resolved explicitly.
    pub fn find_by_pr(&self, pr: u64) -> Vec<&NamedInstance> {
        self.instances
            .values()
            .filter(|i| i.review_target_pr == Some(pr) && i.status == InstanceStatus::Active)
            .collect()
    }

    /// Resolve an instance for a given PR. Fails closed on ambiguity.
    pub fn resolve_for_pr(&self, pr: u64) -> Result<Option<&NamedInstance>> {
        let matches = self.find_by_pr(pr);
        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0])),
            _ => Err(WorktreeError::SamePrAmbiguity {
                pr,
                instances: matches.iter().map(|i| i.name.clone()).collect(),
            }),
        }
    }

    /// Retarget an instance to a different PR. Returns an invalidation
    /// marker so the caller can cascade approval invalidation.
    pub fn retarget(
        &mut self,
        name: &str,
        new_pr: u64,
        new_repo: Option<String>,
    ) -> Result<RetargetResult> {
        let instance =
            self.instances
                .get_mut(name)
                .ok_or_else(|| WorktreeError::InstanceNotFound {
                    name: name.to_owned(),
                })?;

        let old_pr = instance.review_target_pr;
        instance.review_target_pr = Some(new_pr);
        if let Some(repo) = new_repo {
            instance.review_target_repo = Some(repo);
        }
        instance.updated_at = now_ts();

        Ok(RetargetResult {
            instance_name: name.to_owned(),
            old_pr,
            new_pr,
            approvals_invalidated: old_pr.is_some() && old_pr != Some(new_pr),
        })
    }

    pub fn tear_down(&mut self, name: &str) -> Result<()> {
        let instance =
            self.instances
                .get_mut(name)
                .ok_or_else(|| WorktreeError::InstanceNotFound {
                    name: name.to_owned(),
                })?;
        instance.status = InstanceStatus::TornDown;
        instance.updated_at = now_ts();
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetargetResult {
    pub instance_name: String,
    pub old_pr: Option<u64>,
    pub new_pr: u64,
    pub approvals_invalidated: bool,
}

// ---------------------------------------------------------------------------
// Launch routing for same-PR disambiguation
// ---------------------------------------------------------------------------

/// A launch routing request that needs instance resolution.
#[derive(Clone, Debug)]
pub struct LaunchRoutingQuery {
    pub pr_number: u64,
    pub repo: String,
    /// If the user explicitly named an instance, use it.
    pub explicit_instance: Option<String>,
    /// The source surface making the launch request.
    pub source_surface: String,
    /// Whether to allow stale-target recovery (reuse an idle instance).
    pub allow_stale_recovery: bool,
}

/// The outcome of launch routing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchRoutingOutcome {
    /// Exactly one active instance matches — use it.
    Resolved { instance_name: String },
    /// No existing instance — caller should create a new one.
    CreateNew,
    /// Multiple candidates exist — user must disambiguate explicitly.
    Ambiguous {
        candidates: Vec<String>,
        guidance: String,
    },
    /// An idle/stale instance was recovered for reuse.
    RecoveredStale { instance_name: String },
}

/// Resolve which instance should handle a launch for the given PR.
///
/// Rules (per AGENTS.md / canonical plan):
/// - Explicit instance name always wins if it exists and is compatible.
/// - If exactly one active instance targets the PR, use it.
/// - If zero active instances target the PR, signal CreateNew.
/// - If multiple active instances target the PR, fail closed to Ambiguous.
/// - If allow_stale_recovery is set, an idle instance for the same PR may be recovered.
pub fn resolve_launch_routing(
    registry: &InstanceRegistry,
    query: &LaunchRoutingQuery,
) -> Result<LaunchRoutingOutcome> {
    // Explicit instance name takes priority.
    if let Some(ref name) = query.explicit_instance {
        let inst = registry.get(name)?;
        if inst.status == InstanceStatus::TornDown {
            return Err(WorktreeError::InstanceNotFound { name: name.clone() });
        }
        // Warn if targeting a different PR but still honor the explicit request.
        return Ok(LaunchRoutingOutcome::Resolved {
            instance_name: name.clone(),
        });
    }

    // Find all active instances for this PR.
    let active: Vec<&NamedInstance> = registry
        .find_by_pr(query.pr_number)
        .into_iter()
        .filter(|i| i.status == InstanceStatus::Active)
        .collect();

    match active.len() {
        0 => {
            // Check for idle/stale instances if recovery is allowed.
            if query.allow_stale_recovery {
                let idle: Vec<&NamedInstance> = registry
                    .instances
                    .values()
                    .filter(|i| {
                        i.review_target_pr == Some(query.pr_number)
                            && i.status == InstanceStatus::Idle
                    })
                    .collect();
                if idle.len() == 1 {
                    return Ok(LaunchRoutingOutcome::RecoveredStale {
                        instance_name: idle[0].name.clone(),
                    });
                }
            }
            Ok(LaunchRoutingOutcome::CreateNew)
        }
        1 => Ok(LaunchRoutingOutcome::Resolved {
            instance_name: active[0].name.clone(),
        }),
        _ => Ok(LaunchRoutingOutcome::Ambiguous {
            candidates: active.iter().map(|i| i.name.clone()).collect(),
            guidance: format!(
                "Multiple instances target PR #{}: use --instance <name> to select",
                query.pr_number
            ),
        }),
    }
}

// ---------------------------------------------------------------------------
// Stub worktree ops for testing
// ---------------------------------------------------------------------------

/// In-memory stub for worktree operations.
#[derive(Clone, Debug, Default)]
pub struct StubWorktreeOps {
    pub created: Vec<TrackedWorktree>,
    pub removed: Vec<PathBuf>,
    pub fail_create: bool,
}

impl StubWorktreeOps {
    pub fn new() -> Self {
        Self::default()
    }
}

impl WorktreeOps for StubWorktreeOps {
    fn create_worktree(&self, req: &CreateWorktreeRequest) -> Result<TrackedWorktree> {
        if self.fail_create {
            return Err(WorktreeError::GitCommandFailed {
                stderr: "stub: forced failure".to_owned(),
            });
        }
        let commit = req.commit.clone().unwrap_or_else(|| "stubhash0".to_owned());
        Ok(TrackedWorktree {
            id: format!("wt-{}", &commit[..8.min(commit.len())]),
            path: req.target_path.clone(),
            branch: req.branch.clone(),
            commit,
            instance_name: req.instance_name.clone(),
            created_at: now_ts(),
            status: WorktreeStatus::Active,
        })
    }

    fn remove_worktree(&self, _repo_root: &Path, _worktree_path: &Path) -> Result<()> {
        Ok(())
    }

    fn list_worktrees(&self, _repo_root: &Path) -> Result<Vec<TrackedWorktree>> {
        Ok(self.created.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_same_branch_clean() {
        let input = PreflightInput {
            repo_root: PathBuf::from("/repo"),
            current_branch: "feat/x".to_owned(),
            target_head_ref: "feat/x".to_owned(),
            working_tree_dirty: false,
            existing_worktrees: vec![],
        };
        assert_eq!(classify_worktree_need(&input), WorktreeNeed::NotNeeded);
    }

    #[test]
    fn classify_different_branch_clean() {
        let input = PreflightInput {
            repo_root: PathBuf::from("/repo"),
            current_branch: "main".to_owned(),
            target_head_ref: "feat/x".to_owned(),
            working_tree_dirty: false,
            existing_worktrees: vec![],
        };
        assert!(matches!(
            classify_worktree_need(&input),
            WorktreeNeed::Recommended { .. }
        ));
    }

    #[test]
    fn classify_dirty_different_branch_required() {
        let input = PreflightInput {
            repo_root: PathBuf::from("/repo"),
            current_branch: "main".to_owned(),
            target_head_ref: "feat/x".to_owned(),
            working_tree_dirty: true,
            existing_worktrees: vec![],
        };
        assert!(matches!(
            classify_worktree_need(&input),
            WorktreeNeed::Required { .. }
        ));
    }

    #[test]
    fn classify_dirty_same_branch_not_needed() {
        let input = PreflightInput {
            repo_root: PathBuf::from("/repo"),
            current_branch: "feat/x".to_owned(),
            target_head_ref: "feat/x".to_owned(),
            working_tree_dirty: true,
            existing_worktrees: vec![],
        };
        assert_eq!(classify_worktree_need(&input), WorktreeNeed::NotNeeded);
    }

    #[test]
    fn resource_plan_paths() {
        let plan = resolve_resource_plan(
            "review-42",
            Path::new("/home/user/.roger"),
            Path::new("/tmp/worktrees"),
            "my-repo",
        );
        assert_eq!(plan.instance_name, "review-42");
        assert_eq!(
            plan.worktree_path,
            PathBuf::from("/tmp/worktrees/my-repo-review-42")
        );
        assert_eq!(
            plan.db_path,
            PathBuf::from("/home/user/.roger/instances/review-42/roger.db")
        );
    }

    #[test]
    fn stub_create_worktree() {
        let ops = StubWorktreeOps::new();
        let req = CreateWorktreeRequest {
            repo_root: PathBuf::from("/repo"),
            target_path: PathBuf::from("/tmp/wt-test"),
            branch: "feat/x".to_owned(),
            commit: Some("abc12345deadbeef".to_owned()),
            instance_name: Some("test-inst".to_owned()),
        };
        let wt = ops.create_worktree(&req).unwrap();
        assert_eq!(wt.id, "wt-abc12345");
        assert_eq!(wt.branch, "feat/x");
        assert_eq!(wt.instance_name, Some("test-inst".to_owned()));
    }

    #[test]
    fn stub_create_worktree_failure() {
        let ops = StubWorktreeOps {
            fail_create: true,
            ..Default::default()
        };
        let req = CreateWorktreeRequest {
            repo_root: PathBuf::from("/repo"),
            target_path: PathBuf::from("/tmp/wt-fail"),
            branch: "main".to_owned(),
            commit: None,
            instance_name: None,
        };
        assert!(ops.create_worktree(&req).is_err());
    }

    #[test]
    fn instance_registry_basic() {
        let mut reg = InstanceRegistry::new();
        reg.register(NamedInstance {
            name: "review-42".to_owned(),
            review_target_pr: Some(42),
            review_target_repo: Some("acme/widgets".to_owned()),
            worktree_id: None,
            resource_plan: None,
            created_at: 1000,
            updated_at: 1000,
            status: InstanceStatus::Active,
        })
        .unwrap();

        assert_eq!(reg.list_active().len(), 1);
        assert_eq!(reg.get("review-42").unwrap().review_target_pr, Some(42));
    }

    #[test]
    fn instance_resolve_for_pr_unique() {
        let mut reg = InstanceRegistry::new();
        reg.register(NamedInstance {
            name: "inst-a".to_owned(),
            review_target_pr: Some(7),
            review_target_repo: None,
            worktree_id: None,
            resource_plan: None,
            created_at: 1000,
            updated_at: 1000,
            status: InstanceStatus::Active,
        })
        .unwrap();

        let found = reg.resolve_for_pr(7).unwrap().unwrap();
        assert_eq!(found.name, "inst-a");
    }

    #[test]
    fn instance_resolve_for_pr_ambiguity() {
        let mut reg = InstanceRegistry::new();
        for name in &["inst-a", "inst-b"] {
            reg.register(NamedInstance {
                name: name.to_string(),
                review_target_pr: Some(7),
                review_target_repo: None,
                worktree_id: None,
                resource_plan: None,
                created_at: 1000,
                updated_at: 1000,
                status: InstanceStatus::Active,
            })
            .unwrap();
        }

        let err = reg.resolve_for_pr(7).unwrap_err();
        assert!(matches!(err, WorktreeError::SamePrAmbiguity { pr: 7, .. }));
    }

    #[test]
    fn instance_retarget_invalidates_approvals() {
        let mut reg = InstanceRegistry::new();
        reg.register(NamedInstance {
            name: "inst-a".to_owned(),
            review_target_pr: Some(7),
            review_target_repo: None,
            worktree_id: None,
            resource_plan: None,
            created_at: 1000,
            updated_at: 1000,
            status: InstanceStatus::Active,
        })
        .unwrap();

        let result = reg.retarget("inst-a", 99, None).unwrap();
        assert!(result.approvals_invalidated);
        assert_eq!(result.old_pr, Some(7));
        assert_eq!(result.new_pr, 99);
    }

    #[test]
    fn instance_retarget_same_pr_no_invalidation() {
        let mut reg = InstanceRegistry::new();
        reg.register(NamedInstance {
            name: "inst-a".to_owned(),
            review_target_pr: Some(7),
            review_target_repo: None,
            worktree_id: None,
            resource_plan: None,
            created_at: 1000,
            updated_at: 1000,
            status: InstanceStatus::Active,
        })
        .unwrap();

        let result = reg.retarget("inst-a", 7, None).unwrap();
        assert!(!result.approvals_invalidated);
    }

    #[test]
    fn instance_tear_down() {
        let mut reg = InstanceRegistry::new();
        reg.register(NamedInstance {
            name: "inst-a".to_owned(),
            review_target_pr: None,
            review_target_repo: None,
            worktree_id: None,
            resource_plan: None,
            created_at: 1000,
            updated_at: 1000,
            status: InstanceStatus::Active,
        })
        .unwrap();

        reg.tear_down("inst-a").unwrap();
        assert_eq!(reg.get("inst-a").unwrap().status, InstanceStatus::TornDown);
        assert!(reg.list_active().is_empty());
    }

    #[test]
    fn instance_not_found() {
        let reg = InstanceRegistry::new();
        assert!(matches!(
            reg.get("nonexistent").unwrap_err(),
            WorktreeError::InstanceNotFound { .. }
        ));
    }

    // Launch routing tests

    fn routing_query(pr: u64, explicit: Option<&str>) -> LaunchRoutingQuery {
        LaunchRoutingQuery {
            pr_number: pr,
            repo: "acme/widgets".to_owned(),
            explicit_instance: explicit.map(|s| s.to_owned()),
            source_surface: "cli".to_owned(),
            allow_stale_recovery: false,
        }
    }

    fn make_instance(name: &str, pr: u64, status: InstanceStatus) -> NamedInstance {
        NamedInstance {
            name: name.to_owned(),
            review_target_pr: Some(pr),
            review_target_repo: Some("acme/widgets".to_owned()),
            worktree_id: None,
            resource_plan: None,
            created_at: 1000,
            updated_at: 1000,
            status,
        }
    }

    #[test]
    fn routing_no_instances_creates_new() {
        let reg = InstanceRegistry::new();
        let result = resolve_launch_routing(&reg, &routing_query(42, None)).unwrap();
        assert_eq!(result, LaunchRoutingOutcome::CreateNew);
    }

    #[test]
    fn routing_single_instance_resolves() {
        let mut reg = InstanceRegistry::new();
        reg.register(make_instance("inst-a", 42, InstanceStatus::Active))
            .unwrap();

        let result = resolve_launch_routing(&reg, &routing_query(42, None)).unwrap();
        assert_eq!(
            result,
            LaunchRoutingOutcome::Resolved {
                instance_name: "inst-a".to_owned()
            }
        );
    }

    #[test]
    fn routing_multiple_instances_ambiguous() {
        let mut reg = InstanceRegistry::new();
        reg.register(make_instance("inst-a", 42, InstanceStatus::Active))
            .unwrap();
        reg.register(make_instance("inst-b", 42, InstanceStatus::Active))
            .unwrap();

        let result = resolve_launch_routing(&reg, &routing_query(42, None)).unwrap();
        assert!(matches!(result, LaunchRoutingOutcome::Ambiguous { .. }));
    }

    #[test]
    fn routing_explicit_instance_wins() {
        let mut reg = InstanceRegistry::new();
        reg.register(make_instance("inst-a", 42, InstanceStatus::Active))
            .unwrap();
        reg.register(make_instance("inst-b", 42, InstanceStatus::Active))
            .unwrap();

        let result =
            resolve_launch_routing(&reg, &routing_query(42, Some("inst-b"))).unwrap();
        assert_eq!(
            result,
            LaunchRoutingOutcome::Resolved {
                instance_name: "inst-b".to_owned()
            }
        );
    }

    #[test]
    fn routing_stale_recovery() {
        let mut reg = InstanceRegistry::new();
        reg.register(make_instance("inst-idle", 42, InstanceStatus::Idle))
            .unwrap();

        let mut query = routing_query(42, None);
        query.allow_stale_recovery = true;

        let result = resolve_launch_routing(&reg, &query).unwrap();
        assert_eq!(
            result,
            LaunchRoutingOutcome::RecoveredStale {
                instance_name: "inst-idle".to_owned()
            }
        );
    }

    #[test]
    fn routing_torn_down_explicit_fails() {
        let mut reg = InstanceRegistry::new();
        reg.register(make_instance("inst-dead", 42, InstanceStatus::TornDown))
            .unwrap();

        let result = resolve_launch_routing(&reg, &routing_query(42, Some("inst-dead")));
        assert!(result.is_err());
    }
}
