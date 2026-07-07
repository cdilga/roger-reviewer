//! Launch failures must be loud and actionable on the CLI surface.
//!
//! Real-user regression (work feedback, 2026-06-10): "no errors when failing
//! to actually launch review session - it just fails." Every launch failure
//! class must produce a non-zero exit, a clear one-line `error:` reason on
//! stderr, and repair actions rendered as `Try:` lines in human mode, and a
//! `blocked`/`error` robot envelope in robot mode.
//!
//! Failure classes covered here, each driven through the real `rr` binary
//! with fake provider binaries and a fake `gh` on a fully controlled PATH
//! (same pattern as post_contract_smoke.rs / prs_queue_smoke.rs):
//! - provider binary missing
//! - PR not found (gh definitively reports the PR does not exist)
//! - gh missing / gh unauthenticated (loud unverified-target warnings)
//! - preflight ambiguity (missing --pr, ambiguous repo context)
//! - blocked stale/no-match session state on resume
//!
//! Note: the provider-verification failure path (locator/provider mismatch in
//! `verified_provider_session_id`) is not externally triggerable through the
//! CLI because adapters mint locators for the requested provider; it stays a
//! unit-level invariant rather than an integration class.

#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::{TempDir, tempdir};

const PR_NOT_FOUND_STDERR: &str =
    "GraphQL: Could not resolve to a PullRequest with the number of 9999. (repository.pullRequest)";
const GH_UNAUTHENTICATED_STDERR: &str =
    "To get started with GitHub CLI, please run:  gh auth login";

fn rr_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rr"))
}

fn write_executable(path: &Path, script: &str) {
    fs::write(path, script).expect("write fake binary");
    let mut perms = fs::metadata(path)
        .expect("fake binary metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod fake binary");
}

fn write_opencode_stub(dir: &Path) -> PathBuf {
    let path = dir.join("opencode");
    write_executable(
        &path,
        "#!/bin/sh\nif [ \"$1\" = \"export\" ]; then\n  echo \"{}\"\nfi\nexit 0\n",
    );
    path
}

enum FakeGhMode {
    /// `gh pr view` succeeds with a minimal valid PR payload.
    PrFound,
    /// `gh pr view` definitively reports the PR as missing.
    PrNotFound,
    /// `gh` is present but unauthenticated.
    Unauthenticated,
    /// No `gh` binary exists on PATH at all.
    Missing,
}

fn write_gh_stub(dir: &Path, mode: FakeGhMode) {
    let script = match mode {
        FakeGhMode::PrFound => {
            // The fixture PATH contains only this directory, so the stub may
            // use shell builtins only (no cat/sed from /usr/bin).
            r#"#!/bin/sh
printf '%s\n' '{"number":9999,"title":"Fixture PR","state":"OPEN","baseRefName":"main","headRefName":"feature-9999","baseRefOid":"aaa","headRefOid":"bbb","author":{"login":"octocat"},"url":"https://github.com/owner/repo/pull/9999","additions":1,"deletions":1,"changedFiles":1}'
exit 0
"#
            .to_owned()
        }
        FakeGhMode::PrNotFound => {
            format!("#!/bin/sh\necho \"{PR_NOT_FOUND_STDERR}\" >&2\nexit 1\n")
        }
        FakeGhMode::Unauthenticated => {
            format!("#!/bin/sh\necho \"{GH_UNAUTHENTICATED_STDERR}\" >&2\nexit 1\n")
        }
        FakeGhMode::Missing => return,
    };
    write_executable(&dir.join("gh"), &script);
}

struct LaunchFixture {
    _temp: TempDir,
    bin_dir: PathBuf,
    store_root: PathBuf,
    cwd: PathBuf,
    opencode_bin: Option<PathBuf>,
}

fn fixture(gh_mode: FakeGhMode, with_opencode: bool) -> LaunchFixture {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).expect("create fake bin dir");
    write_gh_stub(&bin_dir, gh_mode);
    let opencode_bin = with_opencode.then(|| write_opencode_stub(&bin_dir));

    let store_root = temp.path().join("roger-store");
    let cwd = temp.path().join("workdir");
    fs::create_dir_all(&cwd).expect("create workdir");

    LaunchFixture {
        _temp: temp,
        bin_dir,
        store_root,
        cwd,
        opencode_bin,
    }
}

fn run_rr(fixture: &LaunchFixture, args: &[&str]) -> Output {
    let mut command = Command::new(rr_binary());
    command
        .args(args)
        .current_dir(&fixture.cwd)
        // Fully controlled PATH: only the fake bin dir, so provider binary
        // and gh availability are exactly what each test declares.
        .env("PATH", &fixture.bin_dir)
        .env("RR_STORE_ROOT", &fixture.store_root)
        .env_remove("RR_OPENCODE_BIN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(opencode_bin) = fixture.opencode_bin.as_ref() {
        command.env("RR_OPENCODE_BIN", opencode_bin);
    }
    command.output().expect("run rr binary")
}

fn parse_robot_payload(output: &Output) -> Value {
    let parsed = serde_json::from_slice(&output.stdout);
    assert!(
        parsed.is_ok(),
        "robot payload should be JSON ({}): stdout={} stderr={}",
        parsed
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parsed.expect("JSON validity already asserted")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_loud_human_failure(output: &Output, expected_reason_fragment: &str) {
    let stderr = stderr_text(output);
    assert_ne!(
        output.status.code(),
        Some(0),
        "launch failure must exit non-zero; stdout={} stderr={stderr}",
        stdout_text(output)
    );
    let error_line = stderr
        .lines()
        .find(|line| line.starts_with("error: "))
        .map_or_else(
            || {
                assert!(
                    false,
                    "expected one-line `error:` reason on stderr, got: {stderr}"
                );
                unreachable!()
            },
            |line| line,
        );
    assert!(
        error_line.contains(expected_reason_fragment),
        "error line should name the reason ({expected_reason_fragment}): {error_line}"
    );
    assert!(
        stderr.contains("Try:\n"),
        "repair actions must render as Try: lines on stderr: {stderr}"
    );
}

#[test]
fn review_launch_is_blocked_loudly_when_provider_binary_is_missing() {
    // gh would even confirm the PR; the launch must still fail closed on the
    // missing provider binary instead of claiming "review session launched".
    let fixture = fixture(FakeGhMode::PrFound, false);

    let human = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
        ],
    );
    assert_loud_human_failure(&human, "cannot launch provider 'opencode'");
    assert!(
        stdout_text(&human).contains("provider_binary_missing"),
        "blocked detail should carry the reason code: {}",
        stdout_text(&human)
    );
    assert!(
        !stdout_text(&human).contains("review session launched"),
        "must not claim a launch happened: {}",
        stdout_text(&human)
    );

    let robot = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
            "--robot",
        ],
    );
    assert_eq!(robot.status.code(), Some(3), "{}", stderr_text(&robot));
    let payload = parse_robot_payload(&robot);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "provider_binary_missing");
    assert!(
        !payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .is_empty(),
        "blocked envelope must carry repair actions"
    );
}

#[test]
fn review_launch_is_blocked_loudly_when_pr_is_not_found() {
    let fixture = fixture(FakeGhMode::PrNotFound, true);

    let human = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
        ],
    );
    assert_loud_human_failure(&human, "pull request owner/repo#9999 was not found");
    assert!(
        stdout_text(&human).contains("pr_not_found"),
        "blocked detail should carry the reason code: {}",
        stdout_text(&human)
    );

    let robot = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
            "--robot",
        ],
    );
    assert_eq!(robot.status.code(), Some(3), "{}", stderr_text(&robot));
    let payload = parse_robot_payload(&robot);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "pr_not_found");
}

#[test]
fn review_launch_warns_loudly_when_gh_is_missing() {
    let fixture = fixture(FakeGhMode::Missing, true);

    let human = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
        ],
    );
    assert_eq!(human.status.code(), Some(0), "{}", stderr_text(&human));
    let stderr = stderr_text(&human);
    assert!(
        stderr.contains("could not verify owner/repo#9999")
            && stderr.contains("GitHub CLI (gh) was not found"),
        "gh-missing launch must warn loudly on stderr: {stderr}"
    );

    let robot = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9998",
            "--provider",
            "opencode",
            "--robot",
        ],
    );
    assert_eq!(robot.status.code(), Some(0), "{}", stderr_text(&robot));
    let payload = parse_robot_payload(&robot);
    assert_eq!(payload["data"]["target_verification"], "unverified");
    assert!(
        payload["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .expect("warning text")
                .contains("GitHub CLI (gh) was not found")),
        "robot envelope must carry the unverified-target warning: {payload}"
    );
}

#[test]
fn review_launch_warns_loudly_when_gh_is_unauthenticated() {
    let fixture = fixture(FakeGhMode::Unauthenticated, true);

    let human = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
        ],
    );
    assert_eq!(human.status.code(), Some(0), "{}", stderr_text(&human));
    let stderr = stderr_text(&human);
    assert!(
        stderr.contains("could not verify owner/repo#9999") && stderr.contains("gh auth login"),
        "unauthenticated gh must surface actionable auth guidance: {stderr}"
    );
}

#[test]
fn review_launch_succeeds_quietly_when_target_is_verified() {
    // Sanity guard for the happy path: a verified target with a runnable
    // provider binary launches with exit 0 and no error line.
    let fixture = fixture(FakeGhMode::PrFound, true);

    let human = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
        ],
    );
    let stderr = stderr_text(&human);
    assert_eq!(human.status.code(), Some(0), "{stderr}");
    assert!(
        stdout_text(&human).contains("review session launched"),
        "{}",
        stdout_text(&human)
    );
    assert!(
        !stderr.contains("error: "),
        "verified launch must not emit an error line: {stderr}"
    );

    let robot = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
            "--robot",
        ],
    );
    let payload = parse_robot_payload(&robot);
    assert_eq!(payload["data"]["target_verification"], "verified");
}

#[test]
fn review_preflight_ambiguity_is_loud_when_pr_flag_is_missing() {
    let fixture = fixture(FakeGhMode::PrFound, true);

    let human = run_rr(
        &fixture,
        &["review", "--repo", "owner/repo", "--provider", "opencode"],
    );
    assert_loud_human_failure(&human, "requires --pr");
    assert!(
        stderr_text(&human).contains("pass --pr"),
        "missing-PR ambiguity must point at the repair flag: {}",
        stderr_text(&human)
    );
}

#[test]
fn review_preflight_ambiguity_is_loud_when_repo_context_is_missing() {
    // No --repo, no git on PATH, cwd is not a git repo: target inference is
    // ambiguous and must block loudly instead of guessing.
    let fixture = fixture(FakeGhMode::PrFound, true);

    let human = run_rr(
        &fixture,
        &["review", "--pr", "9999", "--provider", "opencode"],
    );
    assert_loud_human_failure(&human, "repo context inference failed");
}

#[test]
fn resume_blocked_state_is_loud_when_no_session_matches() {
    let fixture = fixture(FakeGhMode::PrFound, true);

    let human = run_rr(
        &fixture,
        &["resume", "--repo", "owner/repo", "--pr", "4242"],
    );
    let stderr = stderr_text(&human);
    assert_ne!(
        human.status.code(),
        Some(0),
        "resume against missing state must exit non-zero; stderr={stderr}"
    );
    assert!(
        stderr.lines().any(|line| line.starts_with("error: ")),
        "resume blocked state must emit a one-line error on stderr: {stderr}"
    );
}

#[test]
fn resume_launch_is_blocked_loudly_when_provider_binary_disappears() {
    // First launch a review with a runnable stub so a session exists, then
    // break the provider binary and confirm interactive resume fails closed
    // instead of pretending to reopen or silently reseeding.
    let fixture = fixture(FakeGhMode::PrFound, true);

    let review = run_rr(
        &fixture,
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "9999",
            "--provider",
            "opencode",
            "--robot",
        ],
    );
    assert_eq!(review.status.code(), Some(0), "{}", stderr_text(&review));

    let broken = LaunchFixture {
        opencode_bin: Some(fixture.bin_dir.join("missing-opencode")),
        ..fixture
    };
    let resume = run_rr(&broken, &["resume", "--repo", "owner/repo", "--pr", "9999"]);
    assert_loud_human_failure(&resume, "cannot launch provider 'opencode'");
    assert!(
        stdout_text(&resume).contains("provider_binary_missing"),
        "blocked resume should carry the reason code: {}",
        stdout_text(&resume)
    );
}
