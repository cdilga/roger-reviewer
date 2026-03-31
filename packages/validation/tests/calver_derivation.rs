use roger_validation::calver::{CalVerChannel, CalVerDerivationInput, derive_calver_release};

#[test]
fn stable_tag_derivation_is_deterministic_for_same_inputs() {
    let input = CalVerDerivationInput {
        git_ref: "refs/tags/v2026.03.31",
        sha: "0123456789abcdef0123456789abcdef01234567",
        run_number: 321,
        run_attempt: 1,
        date_utc: "2026-03-31",
    };

    let first = derive_calver_release(&input).expect("first derivation should succeed");
    let second = derive_calver_release(&input).expect("second derivation should succeed");

    assert_eq!(first, second);
    assert_eq!(first.channel, CalVerChannel::Stable);
    assert_eq!(first.tag, "v2026.03.31");
}

#[test]
fn rc_tag_sets_prerelease_fields() {
    let input = CalVerDerivationInput {
        git_ref: "refs/tags/v2026.03.31-rc.4",
        sha: "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
        run_number: 1001,
        run_attempt: 2,
        date_utc: "2026-03-31",
    };

    let derived = derive_calver_release(&input).expect("rc derivation should succeed");
    assert_eq!(derived.channel, CalVerChannel::Rc);
    assert_eq!(derived.artifact_prefix, "roger-reviewer-2026.03.31-rc.4");
    assert!(derived.release_prerelease);
}

#[test]
fn nightly_derivation_rejects_non_main_branch() {
    let input = CalVerDerivationInput {
        git_ref: "refs/heads/release/0.1.x",
        sha: "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
        run_number: 1001,
        run_attempt: 2,
        date_utc: "2026-03-31",
    };

    let error = derive_calver_release(&input).expect_err("non-main branch should fail");
    assert!(error.contains("unsupported git ref"));
}
