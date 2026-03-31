#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalVerChannel {
    Stable,
    Rc,
    Nightly,
}

impl CalVerChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            CalVerChannel::Stable => "stable",
            CalVerChannel::Rc => "rc",
            CalVerChannel::Nightly => "nightly",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalVerDerivationInput<'a> {
    pub git_ref: &'a str,
    pub sha: &'a str,
    pub run_number: u64,
    pub run_attempt: u32,
    pub date_utc: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalVerDerivation {
    pub channel: CalVerChannel,
    pub canonical_version: String,
    pub artifact_version: String,
    pub tag: String,
    pub release_name: String,
    pub artifact_prefix: String,
    pub release_prerelease: bool,
    pub provenance: String,
}

pub fn derive_calver_release(
    input: &CalVerDerivationInput<'_>,
) -> Result<CalVerDerivation, String> {
    if input.run_number == 0 {
        return Err("run_number must be greater than zero".to_string());
    }
    if input.run_attempt == 0 {
        return Err("run_attempt must be greater than zero".to_string());
    }

    let date_parts = parse_date_parts(input.date_utc)?;
    let canonical_from_date = format_calver_date(date_parts);
    let short_sha = normalize_short_sha(input.sha)?;
    let provenance = format!(
        "sha.{short_sha}.run.{}.attempt.{}",
        input.run_number, input.run_attempt
    );

    if let Some(raw_tag) = input.git_ref.strip_prefix("refs/tags/") {
        return derive_from_tag(raw_tag, &canonical_from_date, provenance);
    }

    if input.git_ref == "refs/heads/main" || input.git_ref == "refs/heads/master" {
        let artifact_version = format!("{canonical_from_date}-nightly.{}", input.run_number);
        return Ok(CalVerDerivation {
            channel: CalVerChannel::Nightly,
            canonical_version: canonical_from_date.clone(),
            artifact_version: artifact_version.clone(),
            tag: format!("v{artifact_version}"),
            release_name: format!("Roger Reviewer {artifact_version}"),
            artifact_prefix: format!("roger-reviewer-{artifact_version}"),
            release_prerelease: true,
            provenance,
        });
    }

    Err(format!(
        "unsupported git ref '{ref}'. expected refs/tags/vYYYY.MM.DD, refs/tags/vYYYY.MM.DD-rc.N, or refs/heads/main",
        ref = input.git_ref
    ))
}

fn derive_from_tag(
    raw_tag: &str,
    canonical_from_date: &str,
    provenance: String,
) -> Result<CalVerDerivation, String> {
    let Some(tag_body) = raw_tag.strip_prefix('v') else {
        return Err(format!(
            "tag '{raw_tag}' must start with 'v' (for example v2026.03.31)"
        ));
    };

    let (channel, core_version, artifact_version) = if let Some((core, rc_seq_raw)) =
        tag_body.split_once("-rc.")
    {
        parse_calver_date(core)?;
        let rc_sequence = rc_seq_raw
            .parse::<u32>()
            .map_err(|_| format!("invalid rc sequence '{rc_seq_raw}'"))?;
        if rc_sequence == 0 {
            return Err("rc sequence must be greater than zero".to_string());
        }
        (
            CalVerChannel::Rc,
            core.to_string(),
            format!("{core}-rc.{rc_sequence}"),
        )
    } else {
        if tag_body.contains('-') {
            return Err(format!(
                "unsupported tag suffix in '{raw_tag}'. allowed suffixes are none (stable) or -rc.N"
            ));
        }
        parse_calver_date(tag_body)?;
        (
            CalVerChannel::Stable,
            tag_body.to_string(),
            tag_body.to_string(),
        )
    };

    if core_version != canonical_from_date {
        return Err(format!(
            "tag date '{core_version}' does not match provided date '{canonical_from_date}'"
        ));
    }

    Ok(CalVerDerivation {
        release_prerelease: channel != CalVerChannel::Stable,
        channel,
        canonical_version: core_version.clone(),
        tag: format!("v{artifact_version}"),
        release_name: format!("Roger Reviewer {artifact_version}"),
        artifact_prefix: format!("roger-reviewer-{artifact_version}"),
        artifact_version,
        provenance,
    })
}

fn parse_date_parts(raw: &str) -> Result<(u16, u8, u8), String> {
    let mut parts = raw.split('-');
    let year = parts
        .next()
        .ok_or_else(|| format!("invalid date '{raw}': expected YYYY-MM-DD"))?;
    let month = parts
        .next()
        .ok_or_else(|| format!("invalid date '{raw}': expected YYYY-MM-DD"))?;
    let day = parts
        .next()
        .ok_or_else(|| format!("invalid date '{raw}': expected YYYY-MM-DD"))?;
    if parts.next().is_some() {
        return Err(format!("invalid date '{raw}': expected YYYY-MM-DD"));
    }

    let year = year
        .parse::<u16>()
        .map_err(|_| format!("invalid year in date '{raw}'"))?;
    let month = month
        .parse::<u8>()
        .map_err(|_| format!("invalid month in date '{raw}'"))?;
    let day = day
        .parse::<u8>()
        .map_err(|_| format!("invalid day in date '{raw}'"))?;

    if month == 0 || month > 12 {
        return Err(format!("invalid month '{month}' in date '{raw}'"));
    }
    if day == 0 || day > 31 {
        return Err(format!("invalid day '{day}' in date '{raw}'"));
    }

    Ok((year, month, day))
}

fn parse_calver_date(raw: &str) -> Result<(u16, u8, u8), String> {
    let mut parts = raw.split('.');
    let year = parts
        .next()
        .ok_or_else(|| format!("invalid calver date '{raw}': expected YYYY.MM.DD"))?;
    let month = parts
        .next()
        .ok_or_else(|| format!("invalid calver date '{raw}': expected YYYY.MM.DD"))?;
    let day = parts
        .next()
        .ok_or_else(|| format!("invalid calver date '{raw}': expected YYYY.MM.DD"))?;
    if parts.next().is_some() {
        return Err(format!("invalid calver date '{raw}': expected YYYY.MM.DD"));
    }

    let date = format!("{year}-{month}-{day}");
    parse_date_parts(&date)
}

fn format_calver_date((year, month, day): (u16, u8, u8)) -> String {
    format!("{year:04}.{month:02}.{day:02}")
}

fn normalize_short_sha(sha: &str) -> Result<String, String> {
    let trimmed = sha.trim();
    if trimmed.len() < 7 {
        return Err(format!(
            "sha '{trimmed}' is too short; expected at least 7 hex characters"
        ));
    }
    if !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!("sha '{trimmed}' must be hex"));
    }
    let normalized = trimmed.to_ascii_lowercase();
    Ok(normalized.chars().take(12).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_stable_tag() {
        let derived = derive_calver_release(&CalVerDerivationInput {
            git_ref: "refs/tags/v2026.03.31",
            sha: "ABCDEF0123456789ABCDEF0123456789ABCDEF01",
            run_number: 77,
            run_attempt: 1,
            date_utc: "2026-03-31",
        })
        .expect("stable tag should derive");

        assert_eq!(derived.channel, CalVerChannel::Stable);
        assert_eq!(derived.tag, "v2026.03.31");
        assert_eq!(derived.artifact_prefix, "roger-reviewer-2026.03.31");
        assert!(!derived.release_prerelease);
        assert_eq!(derived.provenance, "sha.abcdef012345.run.77.attempt.1");
    }

    #[test]
    fn derives_rc_tag() {
        let derived = derive_calver_release(&CalVerDerivationInput {
            git_ref: "refs/tags/v2026.03.31-rc.2",
            sha: "0123456789abcdef0123456789abcdef01234567",
            run_number: 88,
            run_attempt: 3,
            date_utc: "2026-03-31",
        })
        .expect("rc tag should derive");

        assert_eq!(derived.channel, CalVerChannel::Rc);
        assert_eq!(derived.tag, "v2026.03.31-rc.2");
        assert_eq!(derived.release_name, "Roger Reviewer 2026.03.31-rc.2");
        assert!(derived.release_prerelease);
    }

    #[test]
    fn derives_nightly_from_main_branch() {
        let derived = derive_calver_release(&CalVerDerivationInput {
            git_ref: "refs/heads/main",
            sha: "0123456789abcdef0123456789abcdef01234567",
            run_number: 901,
            run_attempt: 2,
            date_utc: "2026-03-31",
        })
        .expect("nightly should derive");

        assert_eq!(derived.channel, CalVerChannel::Nightly);
        assert_eq!(derived.tag, "v2026.03.31-nightly.901");
        assert_eq!(
            derived.artifact_prefix,
            "roger-reviewer-2026.03.31-nightly.901"
        );
        assert!(derived.release_prerelease);
    }

    #[test]
    fn rejects_date_mismatch_between_tag_and_date_input() {
        let err = derive_calver_release(&CalVerDerivationInput {
            git_ref: "refs/tags/v2026.03.31",
            sha: "0123456789abcdef0123456789abcdef01234567",
            run_number: 1,
            run_attempt: 1,
            date_utc: "2026-04-01",
        })
        .expect_err("mismatch should fail");
        assert!(err.contains("does not match provided date"));
    }

    #[test]
    fn rejects_unsupported_branch_ref() {
        let err = derive_calver_release(&CalVerDerivationInput {
            git_ref: "refs/heads/feature/calver",
            sha: "0123456789abcdef0123456789abcdef01234567",
            run_number: 1,
            run_attempt: 1,
            date_utc: "2026-03-31",
        })
        .expect_err("unsupported branch should fail");
        assert!(err.contains("unsupported git ref"));
    }
}
