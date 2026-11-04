//! GitHub Releases as the update source of truth.
//!
//! The in-app update check used to read the signed manifest at
//! `agencyagents.app/updater.json`. It now asks GitHub "what is the
//! latest published release?" and compares that tag with the running
//! build. The signed manifest is still what the installer downloads
//! (see `commands::updater`), but the *version decision* is owned by the
//! published GitHub Release.
//!
//! ## Channels (#23)
//!
//! Two channels, one decision procedure:
//!
//! - **Stable** — `GET /repos/{owner}/{repo}/releases/latest`. It
//!   excludes drafts and pre-releases, matching the "Stable" channel the
//!   UI advertises.
//! - **Beta** — `GET /repos/{owner}/{repo}/releases?per_page=30`, then
//!   the first published (`!draft`) prerelease whose tag is
//!   semver-shaped. When the repo has no published prerelease the beta
//!   channel falls back to the stable `/releases/latest` answer, so
//!   "Beta" is never *older* than "Stable".
//!
//! Both queries share one body fetch (status mapping, rate-limit
//! discrimination, 1 MiB cap) and one parser ([`convert_raw_release`]),
//! so a fix in one path cannot drift from the other.
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

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::commands::settings::UpdateChannel;
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

/// Page size for the beta channel's release list. 30 covers months of
/// releases on this repo, so the newest prerelease is always on the first
/// page without pagination.
const BETA_RELEASE_PAGE_SIZE: usize = 30;

/// A Linux package format a build can be installed from.
///
/// Serializes lowercase (`"deb"` / `"rpm"`) because it reaches the renderer
/// inside [`PackageAsset`], where it picks the download label and the
/// `{format}` placeholder of the explanatory sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageFormat {
    Deb,
    Rpm,
}

impl PackageFormat {
    /// Lowercase name, matching the serde wire form. Used by the
    /// `self_update_unsupported` error message.
    pub fn as_str(self) -> &'static str {
        match self {
            PackageFormat::Deb => "deb",
            PackageFormat::Rpm => "rpm",
        }
    }

    /// The extension that identifies this format among the release assets.
    fn extension(self) -> &'static str {
        match self {
            PackageFormat::Deb => ".deb",
            PackageFormat::Rpm => ".rpm",
        }
    }
}

/// A downloadable installer attached to the release.
///
/// Only the two package formats a Linux build can be installed from are
/// represented: those are the ones whose in-place update path is closed
/// (issue #56), so they are the only artifacts the UI has to link as a
/// manual download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageAsset {
    /// Which package manager installs this asset.
    pub format: PackageFormat,
    /// Asset filename as published, e.g. `Agency.Agents_0.13.0_amd64.deb`.
    /// Shown in the download tooltip so the user knows what they are saving.
    pub name: String,
    /// Validated `https://github.com/...` download URL.
    pub url: String,
}

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
    /// True when GitHub marks the release as a prerelease. The updater
    /// uses this to choose the beta manifest endpoint, and to tell a
    /// beta-channel result apart from the beta fallback to stable.
    pub prerelease: bool,
    /// The `.deb` / `.rpm` installers attached to this release, in asset
    /// order. Empty when the release ships none. The caller selects the
    /// one matching the running build's bundle type; a build that cannot
    /// self-update links this instead of calling the installer (#56).
    pub package_assets: Vec<PackageAsset>,
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
    /// GitHub omits these on some payloads (and pre-2.x fixtures), so
    /// both default to `false`.
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

#[derive(Debug, Deserialize)]
struct RawAsset {
    #[serde(default)]
    name: String,
    /// GitHub's CDN-backed download URL. Absent on assets uploaded in a
    /// state that has not finished processing, so it defaults to empty and
    /// the asset is skipped.
    #[serde(default)]
    browser_download_url: String,
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
    let Some(bytes) = fetch_release_body(client, &url, auth_token).await? else {
        return Ok(None);
    };
    parse_release_json(&bytes, repo)
}

/// Fetch the release the given `channel` should offer.
///
/// - `Stable` → [`fetch_latest_release`].
/// - `Beta` → the newest published prerelease, or the stable latest when
///   the repo has no published prerelease (a beta channel is never older
///   than stable).
///
/// `UpdateChannel` lives in `commands::settings` because it is part of
/// the on-disk/IPC settings schema; duplicating the enum here would
/// create a second definition that could drift from the serde shape the
/// frontend writes.
pub async fn fetch_channel_release(
    client: &reqwest::Client,
    repo: &GithubRepo,
    auth_token: Option<&Token>,
    channel: UpdateChannel,
) -> Result<Option<ReleaseInfo>, AppError> {
    match channel {
        UpdateChannel::Stable => fetch_latest_release(client, repo, auth_token).await,
        UpdateChannel::Beta => {
            if let Some(prerelease) = fetch_newest_prerelease(client, repo, auth_token).await? {
                return Ok(Some(prerelease));
            }
            // No published prerelease anywhere on the first page → the
            // beta channel degrades to stable rather than reporting
            // "up to date" while stable has a newer release.
            fetch_latest_release(client, repo, auth_token).await
        }
    }
}

/// Fetch the newest published prerelease, or `None` when the repo has
/// none on the first page of [`BETA_RELEASE_PAGE_SIZE`] releases.
async fn fetch_newest_prerelease(
    client: &reqwest::Client,
    repo: &GithubRepo,
    auth_token: Option<&Token>,
) -> Result<Option<ReleaseInfo>, AppError> {
    let url = format!(
        "{}/repos/{}/{}/releases?per_page={BETA_RELEASE_PAGE_SIZE}",
        API_BASE, repo.owner, repo.repo
    );
    let Some(bytes) = fetch_release_body(client, &url, auth_token).await? else {
        return Ok(None);
    };
    parse_release_list(&bytes, repo)
}

// ---------- HTTP ----------

/// GET `url` and return the raw body, or `None` when GitHub answers 404.
///
/// One implementation for both channel queries so the status mapping, the
/// rate-limit discrimination, and the body cap cannot drift between the
/// stable release and the prerelease list.
async fn fetch_release_body(
    client: &reqwest::Client,
    url: &str,
    auth_token: Option<&Token>,
) -> Result<Option<Vec<u8>>, AppError> {
    let resp = send_with_optional_auth(client, url, auth_token).await?;

    match resp.status().as_u16() {
        200 => {}
        // No published release yet. Not an error for a young repo, and
        // not an error for a repo whose releases are all drafts.
        404 => return Ok(None),
        403 => return Err(maybe_rate_limited(&resp, url)),
        status => {
            return Err(AppError::HttpStatus {
                url: url.to_string(),
                status,
            });
        }
    }

    read_capped_body(resp, url).await.map(Some)
}

/// Drain the body, refusing anything above [`MAX_RESPONSE_BYTES`] before
/// it reaches a deserializer.
async fn read_capped_body(resp: reqwest::Response, url: &str) -> Result<Vec<u8>, AppError> {
    if let Some(len) = resp.content_length() {
        if len > MAX_RESPONSE_BYTES {
            return Err(AppError::Network {
                url: url.to_string(),
                message: format!("body length {len} exceeds {MAX_RESPONSE_BYTES}"),
            });
        }
    }
    let bytes = resp.bytes().await.map_err(|e| AppError::Network {
        url: url.to_string(),
        message: format!("body: {e}"),
    })?;
    if (bytes.len() as u64) > MAX_RESPONSE_BYTES {
        return Err(AppError::Network {
            url: url.to_string(),
            message: format!("body length {} exceeds {MAX_RESPONSE_BYTES}", bytes.len()),
        });
    }
    Ok(bytes.to_vec())
}

// ---------- Parsing ----------

/// Decode a GitHub response body, tagging the error with the endpoint
/// that produced it so a malformed payload is diagnosable from the toast.
fn decode_json<T: DeserializeOwned>(bytes: &[u8], command: &str) -> Result<T, AppError> {
    serde_json::from_slice(bytes).map_err(|e| AppError::JsonParse {
        command: command.to_string(),
        message: e.to_string(),
        raw_excerpt: String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).into_owned(),
    })
}

/// Pure parser for `/releases/latest`, extracted so the wire shape can be
/// tested without a mock HTTP server. `repo` is used only to build a safe
/// fallback URL when the API's `html_url` is missing or unexpected.
pub(crate) fn parse_release_json(
    bytes: &[u8],
    repo: &GithubRepo,
) -> Result<Option<ReleaseInfo>, AppError> {
    let raw: RawRelease = decode_json(bytes, "releases/latest")?;
    Ok(convert_raw_release(raw, repo))
}

/// Pure parser for `/releases?per_page=N`: the first published prerelease
/// with a semver-shaped tag, or `None`.
///
/// `None` is the signal [`fetch_channel_release`] uses to fall back to the
/// stable release, so drafts and non-semver tags are skipped here rather
/// than surfaced.
fn parse_release_list(bytes: &[u8], repo: &GithubRepo) -> Result<Option<ReleaseInfo>, AppError> {
    let releases: Vec<RawRelease> = decode_json(bytes, "releases")?;
    Ok(releases
        .into_iter()
        .filter(|release| release.prerelease && !release.draft)
        .find_map(|release| convert_raw_release(release, repo)))
}

/// Normalize one raw API release into [`ReleaseInfo`].
///
/// Returns `None` when the tag is not semver-shaped: a repo can publish
/// non-version tags (e.g. `nightly`), and ignoring them is correct — we
/// cannot compare them to the running build.
fn convert_raw_release(raw: RawRelease, repo: &GithubRepo) -> Option<ReleaseInfo> {
    let Some(version) = semver::normalize_tag(&raw.tag_name) else {
        tracing::warn!(
            tag = %raw.tag_name,
            "github releases: release tag is not semver-shaped; ignoring"
        );
        return None;
    };

    let tag = raw.tag_name.trim().to_string();
    let html_url = safe_release_url(raw.html_url.as_deref(), repo, &tag);
    let installable = raw.assets.iter().any(|asset| asset.name == "latest.json");
    let package_assets = package_assets(&raw.assets);

    Some(ReleaseInfo {
        version,
        tag,
        name: bounded_text(raw.name.as_deref(), MAX_NAME_CHARS),
        notes: bounded_text(raw.body.as_deref(), MAX_NOTES_CHARS),
        published_at: raw.published_at,
        html_url,
        installable,
        prerelease: raw.prerelease,
        package_assets,
    })
}

/// The `.deb` / `.rpm` installers among a release's assets.
///
/// An asset qualifies only when its name carries the extension **and** its
/// download URL survives the same https + `github.com` check as the release
/// page: the renderer hands the URL to the OS opener, so a hostile or
/// malformed value must not survive parsing. Anything suspect is dropped
/// rather than repaired — a missing package link degrades to "View release",
/// while a poisoned one is a click away from executing.
fn package_assets(assets: &[RawAsset]) -> Vec<PackageAsset> {
    assets
        .iter()
        .filter_map(|asset| {
            let name = asset.name.as_str();
            let lower = name.to_ascii_lowercase();
            let format = if lower.ends_with(PackageFormat::Deb.extension()) {
                PackageFormat::Deb
            } else if lower.ends_with(PackageFormat::Rpm.extension()) {
                PackageFormat::Rpm
            } else {
                return None;
            };
            if !is_github_https(&asset.browser_download_url) {
                tracing::warn!(
                    asset = %name,
                    "github releases: package asset has an untrusted download URL; ignoring"
                );
                return None;
            }
            Some(PackageAsset {
                format,
                name: name.to_string(),
                url: asset.browser_download_url.clone(),
            })
        })
        .collect()
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
        if is_github_https(url) {
            // `Url::parse` succeeded inside the check; re-parsing to get the
            // normalized form is infallible here.
            if let Ok(parsed) = Url::parse(url) {
                return parsed.to_string();
            }
        }
    }
    format!(
        "https://github.com/{}/{}/releases/tag/{}",
        repo.owner, repo.repo, tag
    )
}

/// True iff `candidate` parses as a credential-free `https://github.com/...`
/// URL. Shared by the release-page check and the package-asset filter so the
/// two cannot drift.
fn is_github_https(candidate: &str) -> bool {
    let Ok(parsed) = Url::parse(candidate) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed.username().is_empty()
        && parsed.password().is_none()
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

    /// One entry of a `/releases` array. `prerelease` / `draft` are
    /// explicit because those two flags decide the beta channel's answer.
    fn raw_release(tag: &str, assets: &[&str], prerelease: bool, draft: bool) -> String {
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
                "assets": [{}],
                "prerelease": {prerelease},
                "draft": {draft}
            }}"#,
            assets_json.join(",")
        )
    }

    fn release_json(tag: &str, assets: &[&str]) -> Vec<u8> {
        raw_release(tag, assets, false, false).into_bytes()
    }

    fn release_list(entries: &[String]) -> Vec<u8> {
        format!("[{}]", entries.join(",")).into_bytes()
    }

    #[test]
    fn parse_extracts_fields_and_normalizes_version() {
        let bytes = release_json(
            "v0.4.0",
            &["latest.json", "Agency_Agents_0.4.0_aarch64.app.tar.gz"],
        );
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
        assert!(!release.prerelease);
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
        assert!(parse_release_json(&bytes, &repo())
            .expect("parse")
            .is_none());
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

    // ---------- Beta channel (#23) ----------

    /// The newest *published* prerelease wins, even when a stable release
    /// sits above it in GitHub's newest-first list.
    #[test]
    fn beta_picks_the_newest_published_prerelease() {
        let bytes = release_list(&[
            raw_release("v0.9.0-beta.2", &["latest.json"], true, false),
            raw_release("v0.8.0", &["latest.json"], false, false),
            raw_release("v0.9.0-beta.1", &["latest.json"], true, false),
        ]);
        let release = parse_release_list(&bytes, &repo())
            .expect("parse")
            .expect("prerelease");
        assert_eq!(release.version, "0.9.0-beta.2");
        assert!(release.prerelease);
    }

    /// A draft prerelease is never offered — it is not published yet, so
    /// its artifacts are not downloadable either.
    #[test]
    fn beta_ignores_draft_prereleases() {
        let bytes = release_list(&[
            raw_release("v0.9.0-beta.3", &["latest.json"], true, true),
            raw_release("v0.9.0-beta.2", &["latest.json"], true, false),
        ]);
        let release = parse_release_list(&bytes, &repo())
            .expect("parse")
            .expect("prerelease");
        assert_eq!(release.version, "0.9.0-beta.2");
    }

    /// A non-semver prerelease tag (e.g. `updater-beta`'s rolling
    /// artifacts) cannot be compared with the running build, so the scan
    /// continues to the next entry.
    #[test]
    fn beta_ignores_non_semver_prereleases() {
        let bytes = release_list(&[
            raw_release("updater-beta", &["latest.json"], true, false),
            raw_release("v0.9.0-beta.1", &["latest.json"], true, false),
        ]);
        let release = parse_release_list(&bytes, &repo())
            .expect("parse")
            .expect("prerelease");
        assert_eq!(release.version, "0.9.0-beta.1");
    }

    /// No published prerelease → `None`, which is exactly the signal
    /// [`fetch_channel_release`] uses to fall back to `/releases/latest`.
    #[test]
    fn beta_without_published_prerelease_reports_none_so_the_caller_falls_back() {
        let bytes = release_list(&[
            raw_release("v0.8.0", &["latest.json"], false, false),
            raw_release("v0.9.0-beta.1", &["latest.json"], true, true),
        ]);
        assert!(parse_release_list(&bytes, &repo())
            .expect("parse")
            .is_none());
    }

    #[test]
    fn beta_rejects_invalid_json_with_the_list_endpoint_tag() {
        let err = parse_release_list(b"[]]", &repo()).expect_err("must fail");
        match err {
            AppError::JsonParse { command, .. } => assert_eq!(command, "releases"),
            other => panic!("expected JsonParse, got {other:?}"),
        }
    }

    /// An empty release list (repo with no releases at all) is `None`,
    /// not an error.
    #[test]
    fn beta_treats_an_empty_release_list_as_none() {
        assert!(parse_release_list(b"[]", &repo()).expect("parse").is_none());
    }

    // ---------- Package assets (#56) ----------

    /// A release whose assets each carry a download URL. `assets` is
    /// `(name, browser_download_url)` so the URL check can be exercised.
    fn release_json_with_downloads(assets: &[(&str, &str)]) -> Vec<u8> {
        let assets_json: Vec<String> = assets
            .iter()
            .map(|(name, url)| {
                format!(r#"{{"name":"{name}","browser_download_url":"{url}"}}"#)
            })
            .collect();
        format!(
            r#"{{
                "tag_name": "v0.13.0",
                "body": "Patch release.",
                "html_url": "https://github.com/deuriib/agency-agents-app/releases/tag/v0.13.0",
                "assets": [{}]
            }}"#,
            assets_json.join(",")
        )
        .into_bytes()
    }

    const DEB_URL: &str = "https://github.com/deuriib/agency-agents-app/releases/download/v0.13.0/Agency.Agents_0.13.0_amd64.deb";
    const RPM_URL: &str = "https://github.com/deuriib/agency-agents-app/releases/download/v0.13.0/Agency.Agents-0.13.0-1.x86_64.rpm";

    /// The two formats whose in-place path the plugin closes are extracted;
    /// everything else on the release (AppImage tarball, `latest.json`, the
    /// `.sig` files) is ignored.
    #[test]
    fn parse_extracts_deb_and_rpm_assets() {
        let bytes = release_json_with_downloads(&[
            ("latest.json", "https://github.com/x/y/releases/download/v0.13.0/latest.json"),
            (
                "Agency.Agents_0.13.0_amd64.AppImage.tar.gz",
                "https://github.com/x/y/releases/download/v0.13.0/AppImage.tar.gz",
            ),
            ("Agency.Agents_0.13.0_amd64.deb", DEB_URL),
            ("Agency.Agents-0.13.0-1.x86_64.rpm", RPM_URL),
            ("Agency.Agents_0.13.0_amd64.deb.sig", "https://github.com/x/y/releases/download/v0.13.0/deb.sig"),
        ]);
        let release = parse_release_json(&bytes, &repo())
            .expect("parse")
            .expect("release");

        assert_eq!(
            release.package_assets,
            vec![
                PackageAsset {
                    format: PackageFormat::Deb,
                    name: "Agency.Agents_0.13.0_amd64.deb".into(),
                    url: DEB_URL.into(),
                },
                PackageAsset {
                    format: PackageFormat::Rpm,
                    name: "Agency.Agents-0.13.0-1.x86_64.rpm".into(),
                    url: RPM_URL.into(),
                },
            ]
        );
    }

    /// A release with no package installers (the AppImage-only case) yields an
    /// empty list rather than an error.
    #[test]
    fn parse_reports_no_package_assets_when_the_release_ships_none() {
        let bytes = release_json_with_downloads(&[(
            "Agency.Agents_0.13.0_amd64.AppImage.tar.gz",
            "https://github.com/x/y/releases/download/v0.13.0/AppImage.tar.gz",
        )]);
        let release = parse_release_json(&bytes, &repo())
            .expect("parse")
            .expect("release");
        assert!(release.package_assets.is_empty());
    }

    /// The download URL is handed to the OS opener, so an asset served from a
    /// host we do not control is dropped, not repaired.
    #[test]
    fn parse_drops_package_assets_with_an_untrusted_download_url() {
        let bytes = release_json_with_downloads(&[
            (
                "Agency.Agents_0.13.0_amd64.deb",
                "https://evil.example/Agency.Agents_0.13.0_amd64.deb",
            ),
            ("Agency.Agents-0.13.0-1.x86_64.rpm", "not a url"),
            // Plain-http github is not good enough either.
            (
                "Agency.Agents_0.13.0_arm64.deb",
                "http://github.com/x/y/releases/download/v0.13.0/a.deb",
            ),
        ]);
        let release = parse_release_json(&bytes, &repo())
            .expect("parse")
            .expect("release");
        assert!(
            release.package_assets.is_empty(),
            "untrusted download URLs must not survive parsing: {:?}",
            release.package_assets
        );
    }

    /// The renderer switches on `format` to pick the download label, so the
    /// lowercase wire form is part of the contract.
    #[test]
    fn package_asset_serializes_lowercase_format_in_camel_case() {
        let asset = PackageAsset {
            format: PackageFormat::Deb,
            name: "Agency.Agents_0.13.0_amd64.deb".into(),
            url: DEB_URL.into(),
        };
        let v = serde_json::to_value(&asset).unwrap();
        assert_eq!(v["format"], "deb");
        assert_eq!(v["name"], "Agency.Agents_0.13.0_amd64.deb");
        assert_eq!(v["url"], DEB_URL);

        let v = serde_json::to_value(PackageAsset {
            format: PackageFormat::Rpm,
            ..asset
        })
        .unwrap();
        assert_eq!(v["format"], "rpm");
    }

    #[test]
    fn package_format_as_str_matches_its_wire_form() {
        assert_eq!(PackageFormat::Deb.as_str(), "deb");
        assert_eq!(PackageFormat::Rpm.as_str(), "rpm");
    }
}
