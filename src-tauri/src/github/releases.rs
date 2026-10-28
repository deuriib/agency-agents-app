//! GitHub Releases as the update source of truth.
//!
//! The in-app update check used to read the signed manifest at
//! `agencyagents.app/updater.json`. It now asks GitHub "what is the
//! latest published release?" and compares that tag with the running
//! build. The signed manifest is still what the installer downloads
//! (see `commands::updater`), but the *version decision* is owned by the
//! published GitHub Release.
//!
//! ## Why `/releases/latest`
//!
//! - It excludes drafts and pre-releases, matching the "Stable" channel
//!   the UI advertises.
//! - It returns `tag_name`, `body`, `published_at`, `html_url`, and the
//!   asset list in one request — enough to render the notice and decide
//!   whether an in-app install is possible.
//! - It is the same endpoint `github::stats` already uses for
//!   `last_release_tag`, so the origin/rate-limit story is unchanged.
//!
//! ## Security posture
//!
//! - Fixed HTTPS origin (`api.github.com`), no caller-supplied URL.
//! - Response body capped before deserialization (1 MiB, shared cap).
//! - `html_url` is only exposed to the renderer after an https +
//!   `github.com` check; anything else falls back to the canonical
//!   release URL built from the validated `owner/repo` + tag.
//! - Release notes are truncated to a bounded number of characters so a
//!   hostile/oversized body cannot bloat the IPC payload.
//! - Non-semver tags are ignored (`Ok(None)`) rather than surfaced as a
//!   bogus version.

use serde::Deserialize;
use url::Url;

use crate::error::AppError;
use crate::github::auth::Token;
use crate::github::stats::{
    maybe_rate_limited, send_with_optional_auth, API_BASE, MAX_RESPONSE_BYTES,
};
use crate::github::url::GithubRepo;
use crate::util::semver;

/// Upper bound on the release-notes string handed to the renderer.
/// Release bodies are editorial, not machine data; 4k characters is
/// plenty for the Settings card and keeps the IPC payload bounded.
const MAX_NOTES_CHARS: usize = 4_000;

/// Upper bound on the release-name string.
const MAX_NAME_CHARS: usize = 200;

/// A published GitHub Release, reduced to the fields the updater needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// Normalized semver without a leading `v` (`v0.4.0` → `0.4.0`).
    /// This is the value compared with `CARGO_PKG_VERSION` and passed
    /// to the installer.
    pub version: String,
    /// Raw `tag_name` as published (`v0.4.0`). Kept for the canonical
    /// release URL and for diagnostics.
    pub tag: String,
    /// Human-readable release name, if the release has one.
    pub name: Option<String>,
    /// Release body (truncated). Rendered as plain text by the UI.
    pub notes: Option<String>,
    /// `published_at` (RFC 3339), if present.
    pub published_at: Option<String>,
    /// Validated `https://github.com/...` release page.
    pub html_url: String,
    /// True when the release carries a `latest.json` asset. Without it
    /// the app can announce the release but cannot install it in-place.
    pub installable: bool,
}

// ---------- Raw API shapes (subset of fields actually used) ----------

#[derive(Debug, Deserialize)]
struct RawRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Debug, Deserialize)]
struct RawAsset {
    #[serde(default)]
    name: String,
}

// ---------- Public API ----------

/// Fetch the latest published release for `repo`.
///
/// `Ok(None)` means "no published release" (404) or "the latest release
/// tag is not semver-shaped". Both are treated as up-to-date by the
/// caller: there is nothing to offer the user.
pub async fn fetch_latest_release(
    client: &reqwest::Client,
    repo: &GithubRepo,
    auth_token: Option<&Token>,
) -> Result<Option<ReleaseInfo>, AppError> {
    let url = format!(
        "{}/repos/{}/{}/releases/latest",
        API_BASE, repo.owner, repo.repo
    );
    let resp = send_with_optional_auth(client, &url, auth_token).await?;

    match resp.status().as_u16() {
        200 => {}
        // No published release yet. Not an error for a young repo, and
        // not an error for a repo whose releases are all drafts.
        404 => return Ok(None),
        403 => return Err(maybe_rate_limited(&resp, &url)),
        status => {
            return Err(AppError::HttpStatus {
                url,
                status,
            });
        }
    }

    if let Some(len) = resp.content_length() {
        if len > MAX_RESPONSE_BYTES {
            return Err(AppError::Network {
                url,
                message: format!("body length {len} exceeds {MAX_RESPONSE_BYTES}"),
            });
        }
    }
    let bytes = resp.bytes().await.map_err(|e| AppError::Network {
        url: url.clone(),
        message: format!("body: {e}"),
    })?;
    if (bytes.len() as u64) > MAX_RESPONSE_BYTES {
        return Err(AppError::Network {
            url,
            message: format!(
                "body length {} exceeds {MAX_RESPONSE_BYTES}",
                bytes.len()
            ),
        });
    }

    parse_release_json(&bytes, repo)
}

/// Pure parser, extracted so the wire shape can be tested without a
/// mock HTTP server. `repo` is used only to build a safe fallback URL
/// when the API's `html_url` is missing or unexpected.
pub(crate) fn parse_release_json(
    bytes: &[u8],
    repo: &GithubRepo,
) -> Result<Option<ReleaseInfo>, AppError> {
    let raw: RawRelease = serde_json::from_slice(bytes).map_err(|e| AppError::JsonParse {
        command: "releases/latest".into(),
        message: e.to_string(),
        raw_excerpt: String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).into_owned(),
    })?;

    let Some(version) = semver::normalize_tag(&raw.tag_name) else {
        // A repo can publish non-version tags (e.g. `nightly`). Ignoring
        // them is correct: we cannot compare them to the running build.
        tracing::warn!(
            tag = %raw.tag_name,
            "github releases: latest release tag is not semver-shaped; ignoring"
        );
        return Ok(None);
    };

    let tag = raw.tag_name.trim().to_string();
    let html_url = safe_release_url(raw.html_url.as_deref(), repo, &tag);
    let installable = raw.assets.iter().any(|asset| asset.name == "latest.json");

    Ok(Some(ReleaseInfo {
        version,
        tag,
        name: bounded_text(raw.name.as_deref(), MAX_NAME_CHARS),
        notes: bounded_text(raw.body.as_deref(), MAX_NOTES_CHARS),
        published_at: raw.published_at,
        html_url,
        installable,
    }))
}

/// Trim, collapse a trailing newline, and truncate on a char boundary.
/// Returns `None` for empty/whitespace-only input.
fn bounded_text(value: Option<&str>, max_chars: usize) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    if trimmed.chars().count() > max_chars {
        out.push('…');
    }
    Some(out)
}

/// Only expose an `https://github.com/...` release page to the renderer.
/// The API is TLS-trusted, but `safeOpenUrl` will hand the value to the
/// OS opener; a host check is cheap defense-in-depth. On any mismatch we
/// build the canonical URL from the validated repo + tag instead.
fn safe_release_url(candidate: Option<&str>, repo: &GithubRepo, tag: &str) -> String {
    if let Some(url) = candidate {
        if let Ok(parsed) = Url::parse(url) {
            let is_github = parsed.scheme() == "https"
                && parsed.host_str() == Some("github.com")
                && parsed.username().is_empty()
                && parsed.password().is_none();
            if is_github {
                return parsed.to_string();
            }
        }
    }
    format!(
        "https://github.com/{}/{}/releases/tag/{}",
        repo.owner, repo.repo, tag
    )
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> GithubRepo {
        GithubRepo {
            owner: "deuriib".into(),
            repo: "agency-agents-app".into(),
        }
    }

    fn release_json(tag: &str, assets: &[&str]) -> Vec<u8> {
        let assets_json: Vec<String> = assets
            .iter()
            .map(|name| format!(r#"{{"name":"{name}","size":1}}"#))
            .collect();
        format!(
            r#"{{
                "tag_name": "{tag}",
                "name": "Agency Agents {tag}",
                "body": "  Fixed the updater.\n",
                "published_at": "2026-10-27T12:00:00Z",
                "html_url": "https://github.com/deuriib/agency-agents-app/releases/tag/{tag}",
                "assets": [{}]
            }}"#,
            assets_json.join(",")
        )
        .into_bytes()
    }

    #[test]
    fn parse_extracts_fields_and_normalizes_version() {
        let bytes = release_json("v0.4.0", &["latest.json", "Agency_Agents_0.4.0_aarch64.app.tar.gz"]);
        let release = parse_release_json(&bytes, &repo())
            .expect("parse")
            .expect("release");

        assert_eq!(release.version, "0.4.0");
        assert_eq!(release.tag, "v0.4.0");
        assert_eq!(release.name.as_deref(), Some("Agency Agents v0.4.0"));
        assert_eq!(release.notes.as_deref(), Some("Fixed the updater."));
        assert_eq!(
            release.published_at.as_deref(),
            Some("2026-10-27T12:00:00Z")
        );
        assert_eq!(
            release.html_url,
            "https://github.com/deuriib/agency-agents-app/releases/tag/v0.4.0"
        );
        assert!(release.installable);
    }

    #[test]
    fn parse_marks_not_installable_without_latest_json() {
        let bytes = release_json("v0.4.0", &["Agency_Agents_0.4.0_aarch64.dmg"]);
        let release = parse_release_json(&bytes, &repo())
            .expect("parse")
            .expect("release");
        assert!(!release.installable);
    }

    #[test]
    fn parse_ignores_non_semver_tags() {
        let bytes = release_json("nightly", &["latest.json"]);
        assert!(parse_release_json(&bytes, &repo()).expect("parse").is_none());
    }

    #[test]
    fn parse_accepts_bare_semver_tag() {
        let bytes = release_json("0.4.0", &["latest.json"]);
        let release = parse_release_json(&bytes, &repo())
            .expect("parse")
            .expect("release");
        assert_eq!(release.version, "0.4.0");
        assert_eq!(release.tag, "0.4.0");
    }

    #[test]
    fn parse_falls_back_when_html_url_is_not_github() {
        let bytes = br#"{
            "tag_name": "v0.4.0",
            "html_url": "http://evil.example/releases/tag/v0.4.0",
            "assets": [{"name":"latest.json"}]
        }"#;
        let release = parse_release_json(bytes, &repo())
            .expect("parse")
            .expect("release");
        assert_eq!(
            release.html_url,
            "https://github.com/deuriib/agency-agents-app/releases/tag/v0.4.0"
        );
    }

    #[test]
    fn parse_truncates_release_notes() {
        let body = "x".repeat(MAX_NOTES_CHARS + 50);
        let bytes = format!(
            r#"{{"tag_name":"v0.4.0","body":"{body}","assets":[{{"name":"latest.json"}}]}}"#
        )
        .into_bytes();
        let release = parse_release_json(&bytes, &repo())
            .expect("parse")
            .expect("release");
        let notes = release.notes.expect("notes");
        assert_eq!(notes.chars().count(), MAX_NOTES_CHARS + 1);
        assert!(notes.ends_with('…'));
    }

    #[test]
    fn parse_returns_none_for_empty_notes() {
        let bytes = br#"{"tag_name":"v0.4.0","body":"   ","assets":[]}"#;
        let release = parse_release_json(bytes, &repo())
            .expect("parse")
            .expect("release");
        assert!(release.notes.is_none());
        assert!(release.name.is_none());
    }

    #[test]
    fn parse_rejects_invalid_json() {
        let err = parse_release_json(b"{not json", &repo()).expect_err("must fail");
        match err {
            AppError::JsonParse { command, .. } => assert_eq!(command, "releases/latest"),
            other => panic!("expected JsonParse, got {other:?}"),
        }
    }
}
