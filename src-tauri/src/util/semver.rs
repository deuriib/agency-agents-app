//! Minimal three-tuple semver parsing + strict-upgrade comparison.
//!
//! Two subsystems need the same answer to "is this tag newer than the
//! running build?":
//!
//! - `commands::updater` — the explicit downgrade-attack defense.
//! - `github::releases` — turning a GitHub release tag (`v0.4.0`) into a
//!   comparable version and rejecting non-semver tags.
//!
//! Keeping one parser here means the release check and the install guard
//! can never disagree about what `v0.4.0-beta.1` means.
//!
//! We intentionally do **not** pull in a full semver crate: our release
//! cadence is numeric `major.minor.patch` only, and pre-release/build
//! suffixes are stripped for comparison. The behavioral contract is
//! pinned by the tests below.

/// Parse a semver-shaped string into `(major, minor, patch)`.
///
/// A leading `v`/`V` is accepted (GitHub release tags conventionally
/// carry one). Pre-release / build metadata suffixes are dropped
/// (`0.3.1-beta.1` → `(0, 3, 1)`). Anything else returns `None`.
pub fn parse(s: &str) -> Option<(u32, u32, u32)> {
    let trimmed = s.trim().trim_start_matches(['v', 'V']);
    let mut iter = trimmed.splitn(3, '.');
    let major: u32 = iter.next()?.parse().ok()?;
    let minor: u32 = iter.next()?.parse().ok()?;
    let patch_raw = iter.next()?;
    // Drop pre-release / metadata suffix (`0.3.1-beta.1`).
    let patch_str = patch_raw
        .split(['-', '+'])
        .next()
        .unwrap_or(patch_raw);
    let patch: u32 = patch_str.parse().ok()?;
    Some((major, minor, patch))
}

/// Normalize a release tag to the bare version string used by the
/// updater manifest (`v0.4.0` → `0.4.0`). Returns `None` when the tag is
/// not semver-shaped, so callers can ignore non-version tags instead of
/// surfacing a bogus update.
pub fn normalize_tag(tag: &str) -> Option<String> {
    let trimmed = tag.trim().trim_start_matches(['v', 'V']);
    if trimmed.is_empty() || parse(trimmed).is_none() {
        return None;
    }
    Some(trimmed.to_string())
}

/// True when `target` is a strict upgrade over `current`.
///
/// Unparseable inputs return `false`: refusing the upgrade is safer than
/// installing a binary we cannot compare.
pub fn is_strict_upgrade(current: &str, target: &str) -> bool {
    match (parse(current), parse(target)) {
        (Some(cur), Some(tgt)) => tgt > cur,
        _ => false,
    }
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trips_basic() {
        assert_eq!(parse("0.3.1"), Some((0, 3, 1)));
        assert_eq!(parse("1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse("12.345.6789"), Some((12, 345, 6789)));
    }

    #[test]
    fn parse_strips_prerelease() {
        assert_eq!(parse("0.3.1-beta.1"), Some((0, 3, 1)));
        assert_eq!(parse("0.3.1+build.7"), Some((0, 3, 1)));
    }

    #[test]
    fn parse_accepts_v_prefix() {
        assert_eq!(parse("v0.3.1"), Some((0, 3, 1)));
        assert_eq!(parse("V1.2.3"), Some((1, 2, 3)));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert_eq!(parse("not a version"), None);
        assert_eq!(parse("1.2"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn normalize_tag_returns_bare_version() {
        assert_eq!(normalize_tag("v0.4.0").as_deref(), Some("0.4.0"));
        assert_eq!(normalize_tag("0.4.0").as_deref(), Some("0.4.0"));
        assert_eq!(normalize_tag(" v0.4.0 ").as_deref(), Some("0.4.0"));
        assert!(normalize_tag("release-2026").is_none());
        assert!(normalize_tag("").is_none());
    }

    #[test]
    fn is_strict_upgrade_basic() {
        assert!(is_strict_upgrade("0.3.0", "0.3.1"));
        assert!(is_strict_upgrade("0.3.0", "0.4.0"));
        assert!(is_strict_upgrade("0.3.0", "1.0.0"));
    }

    #[test]
    fn is_strict_upgrade_rejects_same_or_older() {
        assert!(!is_strict_upgrade("0.3.0", "0.3.0"));
        assert!(!is_strict_upgrade("0.3.1", "0.3.0"));
        assert!(!is_strict_upgrade("1.0.0", "0.99.99"));
    }

    #[test]
    fn is_strict_upgrade_handles_v_prefix() {
        assert!(is_strict_upgrade("v0.3.0", "v0.3.1"));
        assert!(is_strict_upgrade("0.3.0", "v0.3.1"));
    }

    #[test]
    fn is_strict_upgrade_unparseable_rejects() {
        assert!(!is_strict_upgrade("garbage", "0.3.1"));
        assert!(!is_strict_upgrade("0.3.0", "also garbage"));
    }
}
