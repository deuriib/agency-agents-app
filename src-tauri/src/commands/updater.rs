//! Phase 15 — in-app updater commands.
//!
//! ## Version decision vs. installation
//!
//! The two halves of the updater have different sources of truth:
//!
//! - **Is there a newer version?** → `github::releases::fetch_latest_release`
//!   reads `GET /repos/deuriib/agency-agents-app/releases/latest`. The
//!   published GitHub Release is the source of truth; the app compares
//!   its tag with `CARGO_PKG_VERSION`.
//! - **How is it installed?** → `tauri-plugin-updater` downloads the
//!   signed `latest.json` manifest attached to that same release,
//!   verifies the artifact (sha256 + minisign), and swaps the bundle.
//!
//! Keeping the check independent of the manifest means a release that
//! ships without `latest.json` can still be announced (the UI links to
//! the release page) instead of failing silently. `ReleaseInfo.installable`
//! carries that distinction across the IPC boundary.
//!
//! ## Defense-in-depth gates
//!
//! 1. **Paranoid mode** — every IPC entry consults
//!    `require_network("update_check")` so the master Offline-Mode switch
//!    kills the update path before any outbound request.
//! 2. **Stale-version sanity check** — `update_install(version)` validates
//!    that `version` matches the most recently-seen `Available` entry on
//!    `AppState.updater_state`. If the UI requested an install of a
//!    version we no longer have in memory (e.g. the user kept Settings
//!    open while a newer release landed via the auto-check), the call
//!    returns `AppError::InvalidArgument` rather than installing the
//!    wrong thing.
//! 3. **Explicit downgrade rejection** — refuses to install a target whose
//!    semver is less-than-or-equal to the running version. The plugin
//!    already refuses semver-older targets via its default comparator,
//!    but we re-check here so a future plugin behaviour change cannot
//!    reopen the hole.
//! 4. **Installability gate** — a release without a `latest.json` asset is
//!    announced but not installable. `update_install` refuses it with a
//!    typed error instead of handing the plugin a manifest that isn't
//!    there.
//!
//! ## Background auto-install (Phase 3)
//!
//! Two independent opt-ins must both hold: `update_auto_install` is on (explicit
//! consent, prompted the first time in Settings) *and* Offline Mode is off. When
//! they do, a successful auto-check hands the discovered version to
//! [`run_auto_install`], which re-reads the live settings, honours the
//! skip-list, and enforces single-flight. The download is split from the
//! install so a cancel — consent revoked, Offline Mode flipped on, or the
//! explicit cancel command — can land after signature verification but before
//! the bundle swap, leaving the disk untouched.
//!
//! Every step emits [`AUTO_INSTALL_EVENT`]: `started`, `verified`, then one of
//! `installed` / `failed` / `canceled`. The same facts go to the tracing log as
//! the audit trail. The relaunch is never automatic — `update_relaunch` stays
//! user-gated, so an unattended install can never restart the app under the
//! user's feet.
//!
//! **Discovery boundary.** The scheduler is the only trigger for auto-install: a
//! manual "Check for updates now" reports the outcome and never starts an
//! install, even with the toggle on. The manual button means "tell me"; the
//! toggle means "handle it unattended". The scheduler's first wake is
//! [`AUTO_CHECK_STARTUP_DELAY`] after launch, then [`AUTO_CHECK_INTERVAL`] while
//! the process stays up — `last_checked_at` is memory-only, so a first wake of
//! one full interval meant a user who restarts the app daily never got a check
//! at all, and therefore never an install.
//!
//! The plugin's verify-then-install pipeline is the load-bearing crypto
//! chokepoint; our wrapper surfaces typed errors when it fails so the
//! frontend can route the toast through the same channel as every other
//! typed error.
//!
//! ## Testability
//!
//! Real plugin invocations require a `tauri::App` and a live network
//! call, neither of which we want in unit tests. Two small traits are
//! the abstraction:
//!
//! - [`ReleaseSource`] — "what does the release channel say?"
//! - [`InstallBackend`] — "download + verify + install this version."
//!
//! Production uses [`GithubReleaseSource`] and [`PluginBackend`]; tests
//! use the mocks in this module's `tests` submodule. Mirrors the
//! `KeychainSlot` pattern in `github::auth`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::RwLock;

use crate::error::AppError;
use crate::github::auth::Token;
use crate::github::{self, GithubRepo, ReleaseInfo};
use crate::state::AppState;
use crate::util::semver;

// ---------- IPC payloads ----------

/// Outcome of [`update_check_now`]. Mirrors the three real states the
/// plugin can return, flattened into a single discriminated union the
/// frontend can `switch` over.
///
/// The `Blocked` variant is **not** returned by this enum — paranoid
/// mode surfaces as `Err(AppError::ParanoidModeBlocked)` instead, so
/// the toast routes through the same channel as every other gated call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpdateCheckOutcome {
    /// Plugin returned no update — running version is current.
    UpToDate,
    /// Plugin returned an update. Fields the UI needs to render the
    /// indicator pill + Settings card.
    Available {
        /// Announced version (semver) — used by `update_install` as
        /// the sanity-check arg.
        version: String,
        /// Currently-installed version, surfaced so the UI can render
        /// "v0.3.0 → v0.3.1" without an extra IPC call.
        current_version: String,
        /// Release-notes body from the manifest. Optional.
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
        /// Publish date (RFC 3339), if present in the release.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub_date: Option<String>,
        /// Validated `https://github.com/...` release page. The UI uses
        /// this for the "Release notes" link and for the "View release"
        /// action when the release is not installable in-app.
        release_url: String,
        /// True when the release carries a `latest.json` asset. False
        /// means the UI should offer "View release" (manual download)
        /// instead of "Install update".
        installable: bool,
        /// True iff the user has already skipped this version via the
        /// title-bar indicator's `×`. UI uses this to suppress the
        /// re-display of the indicator (Settings panel still shows the
        /// card so the user can install if they change their mind).
        skipped: bool,
    },
}

/// Subset of a published release we cache for the `update_install`
/// stale-version sanity check + the auto-check scheduler's "last
/// available" state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedUpdate {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub release_url: String,
    pub installable: bool,
}

// ---------- State ----------

/// Cooperative cancellation flag for a background auto-install.
///
/// Shared by reference (cheap `Arc` clone) between three owners that cannot
/// otherwise talk to each other without a lock:
///
/// 1. the scheduler, which arms a fresh token before each attempt;
/// 2. `settings_set`, which requests cancellation the moment the user turns
///    auto-install off (consent revocation must not wait for a 24h wake);
/// 3. `PluginBackend`, which consults it **after** the artifact is downloaded
///    and signature-verified but **before** the bundle swap — the last point
///    where aborting leaves the disk untouched.
///
/// Deliberately an atomic rather than a `tokio::sync` primitive: the check
/// happens inside a synchronous callback slot in the plugin pipeline, so it
/// must not need `.await`.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the in-flight install (if any) to stop before the bundle swap.
    /// Idempotent — repeated requests are free.
    pub fn request(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Clear the flag. Called by the scheduler immediately before it starts a
    /// new attempt so a cancel from a previous cycle cannot leak forward.
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Lifecycle of the background auto-install, mirrored for the UI and used as
/// the scheduler's in-flight guard (ENG-11: at most one install at a time).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AutoInstallStatus {
    /// No background install has run since launch.
    #[default]
    Idle,
    /// Download + verify + install in progress for `version`.
    Running { version: String },
    /// Installed; waiting for the user to accept the relaunch prompt.
    Installed { version: String },
    /// Attempt failed. `message` is the typed error text shown in Settings.
    Failed { version: String, message: String },
    /// Aborted before the bundle swap (consent revoked, Offline Mode on, or
    /// explicit cancel). `cached_available` is left intact for a manual retry.
    Canceled { version: String },
}

/// In-memory mirror of the latest update check result. Stored on
/// `AppState.updater_state` so the auto-check scheduler and the
/// `update_install` validator share the same view.
#[derive(Debug, Default)]
pub struct UpdaterState {
    /// Latest plugin outcome, if a check has run.
    pub last_outcome: Option<UpdateCheckOutcome>,
    /// Unix timestamp (seconds) of the most recent successful check.
    /// Used by the scheduler's 24h-floor enforcement so cross-launch
    /// behaviour is predictable.
    pub last_checked_at: Option<i64>,
    /// Cached `Available` payload for the install-arg sanity check.
    /// Cleared when the outcome flips back to `UpToDate`.
    pub cached_available: Option<CachedUpdate>,
    /// Background auto-install lifecycle. `Running` doubles as the
    /// single-flight guard for the scheduler.
    pub auto_install: AutoInstallStatus,
    /// Cancellation flag for an in-flight background install.
    pub auto_install_cancel: CancelToken,
}

// ---------- Backend abstractions ----------

/// "What does the release channel say?" abstraction. Production uses
/// [`GithubReleaseSource`]; tests use `MockReleaseSource`.
///
/// The check is deliberately independent of the signed manifest: a
/// release can exist without a `latest.json` asset, and the UI should
/// still be able to announce it and link to the release page.
#[async_trait::async_trait]
pub trait ReleaseSource: Send + Sync {
    /// Latest published release, or `None` when there is no published
    /// release and when the tag is not semver-shaped.
    async fn latest(&self) -> Result<Option<ReleaseInfo>, AppError>;
}

/// "Download + verify + install this version" abstraction around
/// `tauri-plugin-updater`. Production uses [`PluginBackend`]; tests use
/// `MockInstallBackend`.
#[async_trait::async_trait]
pub trait InstallBackend: Send + Sync {
    /// Download + verify (sha256 + minisign) + install the update. The
    /// `version` arg is for sanity checking only; the plugin uses its
    /// own internal `Update` handle (cached from its own manifest check)
    /// so we don't have to round-trip the full Update state through the
    /// trait boundary.
    async fn download_and_install(&self, version: &str) -> Result<(), AppError>;
}

/// Currently-running app version, surfaced for downgrade rejection
/// without bringing the plugin into the public API. Resolved once at
/// startup from `CARGO_PKG_VERSION`.
pub fn current_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ---------- Production release source ----------

/// GitHub Releases API adapter. Holds the reqwest client + optional
/// token so the scheduler can build it once per wake without re-reading
/// the Keychain inside the request path.
pub struct GithubReleaseSource {
    client: reqwest::Client,
    repo: GithubRepo,
    token: Option<Token>,
}

impl GithubReleaseSource {
    pub fn new(client: reqwest::Client, repo: GithubRepo, token: Option<Token>) -> Self {
        Self {
            client,
            repo,
            token,
        }
    }
}

#[async_trait::async_trait]
impl ReleaseSource for GithubReleaseSource {
    async fn latest(&self) -> Result<Option<ReleaseInfo>, AppError> {
        github::fetch_latest_release(&self.client, &self.repo, self.token.as_ref()).await
    }
}

/// Build the production release source: fixed app repo, the shared
/// GitHub client, and an optional Keychain token.
///
/// A Keychain failure must not break update checks — anonymous access is
/// enough for one request per day — so the error is logged and the call
/// proceeds anonymously. This is the one place where an auth failure is
/// downgraded instead of surfaced: the alternative is an update check
/// that fails because the user's keychain is locked, which is worse.
pub fn build_release_source() -> Result<GithubReleaseSource, AppError> {
    let client = github::stats::build_client()?;
    let token = match github::auth::read_token() {
        Ok(token) => token,
        Err(e) => {
            tracing::warn!(
                "updater: keychain token unavailable; using anonymous GitHub API: {e:?}"
            );
            None
        }
    };
    Ok(GithubReleaseSource::new(client, github::app_repo(), token))
}

// ---------- Production install backend ----------

/// Production backend that delegates to `tauri-plugin-updater`.
///
/// The plugin's `UpdaterExt::updater().check().await` returns a typed
/// `Update` value or `None`. We translate any failure into our typed
/// `AppError` family so the IPC contract stays uniform across the
/// surface.
#[cfg(not(test))]
pub struct PluginBackend<R: tauri::Runtime> {
    // app: tauri::AppHandle<R>,
    /// Consulted between download-verify and the bundle swap.
    cancel: CancelToken,
    /// Notified once the artifact is downloaded AND signature-verified,
    /// immediately before the install swaps the bundle. `None` on the manual
    /// path so a user-initiated install emits no background-install events.
    on_verified: Option<Box<dyn Fn() + Send + Sync>>,
}

#[cfg(not(test))]
impl<R: tauri::Runtime> PluginBackend<R> {
    /// Manual-install backend: nothing can cancel it and it emits no events.
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        Self {
            app,
            cancel: CancelToken::new(),
            on_verified: None,
        }
    }

    /// Background-install backend: cancellable, and reports the
    /// "downloaded + verified" milestone so the scheduler can emit it.
    pub fn auto(
        app: tauri::AppHandle<R>,
        cancel: CancelToken,
        on_verified: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            app,
            cancel,
            on_verified: Some(Box::new(on_verified)),
        }
    }

    /// Borrow the plugin's `Updater` value. Built fresh on each call —
    /// the plugin is cheap to instantiate and doing it eagerly at
    /// startup would force the manifest endpoint validation into the
    /// setup hook (failing builds with a malformed endpoint).
    fn updater(&self) -> Result<tauri_plugin_updater::Updater, AppError> {
        use tauri_plugin_updater::UpdaterExt;
        self.app
            .updater()
            .map_err(|e| AppError::Internal {
                message: format!("updater plugin init: {e}"),
            })
    }
}

#[cfg(not(test))]
#[async_trait::async_trait]
impl<R: tauri::Runtime> InstallBackend for PluginBackend<R> {
    async fn download_and_install(&self, version: &str) -> Result<(), AppError> {
        let updater = self.updater()?;
        let opt = updater
            .check()
            .await
            .map_err(|e| translate_plugin_error(e, "update check (pre-install)"))?;
        let Some(update) = opt else {
            // Manifest no longer advertises an update. The frontend
            // requested an install but the manifest changed underneath
            // us; surface as InvalidArgument so the UI can refresh.
            return Err(AppError::InvalidArgument {
                message: format!(
                    "manifest no longer advertises update for {version}; refresh and retry"
                ),
            });
        };

        // Re-check version match. The cached `update_install` arg
        // validator in `update_install` runs first against
        // `AppState.updater_state.cached_available`; this is a second
        // line of defense at the plugin boundary.
        if update.version != version {
            return Err(AppError::InvalidArgument {
                message: format!(
                    "manifest version drifted: requested {version}, manifest reports {}",
                    update.version
                ),
            });
        }

        // Download and install are split on purpose. `Update::download`
        // fetches the artifact AND runs the minisign verification;
        // `Update::install` only swaps the bundle. Splitting them puts the
        // cancellation check exactly where it belongs: after the bytes are
        // proven authentic, before anything on disk changes. A cancelled
        // download therefore leaves no partial artifact and no swapped
        // bundle — the cached "available" entry stays valid for a manual
        // retry.
        let bytes = update
            .download(|_, _| {}, || {})
            .await
            .map_err(|e| translate_plugin_error(e, "update download"))?;

        // Cancellation checkpoint: after the bytes are proven authentic, before
        // anything on disk changes. Checked *before* the `verified`
        // notification so that phase means "verified and proceeding" — notifying
        // first told the UI "signature verified, installing…" and then went
        // silent when the cancel landed.
        if self.cancel.is_requested() {
            tracing::info!(
                "updater: install of {version} canceled after verification; discarding artifact"
            );
            return Err(AppError::InstallCanceled);
        }

        if let Some(notify) = &self.on_verified {
            notify();
        }

        update
            .install(bytes)
            .map_err(|e| translate_plugin_error(e, "update install"))?;

        Ok(())
    }
}

/// Map a plugin `Error` onto our typed `AppError` family. The plugin's
/// own error type carries enough context to discriminate hash mismatch
/// from signature failure, but its `Display` string is the most reliable
/// classifier across versions (the variants change between minor
/// releases).
#[cfg(not(test))]
fn translate_plugin_error(e: tauri_plugin_updater::Error, context: &str) -> AppError {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    // Signature / minisign failures map to `SignatureVerificationFailed`
    // so the UI surfaces the same toast as a sha256 mismatch (both are
    // "the bytes did not verify, abort").
    if lower.contains("signature") || lower.contains("minisign") || lower.contains("pubkey") {
        AppError::SignatureVerificationFailed { message: msg }
    } else {
        AppError::Network {
            url: context.to_string(),
            message: msg,
        }
    }
}

// ---------- Public API ----------

/// Inner: ask the release channel for the latest published release and
/// turn it into an [`UpdateCheckOutcome`], caching the result on
/// `state.updater_state`. Extracted so the auto-check scheduler can
/// call it without going through the IPC layer.
///
/// Only a strictly newer version is surfaced. A same-version release
/// (re-published asset), an older release (rollback), no published
/// release, and a non-semver tag all collapse to `UpToDate`: the app
/// never offers a downgrade affordance.
pub async fn run_check(
    state: &AppState,
    source: &dyn ReleaseSource,
) -> Result<UpdateCheckOutcome, AppError> {
    state.require_network("update_check").await?;
    let latest = source.latest().await?;
    let now = chrono::Utc::now().timestamp();
    let current = current_app_version();

    let outcome = match latest {
        Some(release) if semver::is_strict_upgrade(current, &release.version) => {
            let skipped = is_version_skipped(state, &release.version).await;
            UpdateCheckOutcome::Available {
                version: release.version,
                current_version: current.to_string(),
                notes: release.notes,
                pub_date: release.published_at,
                release_url: release.html_url,
                installable: release.installable,
                skipped,
            }
        }
        _ => UpdateCheckOutcome::UpToDate,
    };

    // Persist into AppState so subsequent install requests can validate.
    {
        let mut guard = state.updater_state.write().await;
        guard.last_outcome = Some(outcome.clone());
        guard.last_checked_at = Some(now);
        guard.cached_available = match &outcome {
            UpdateCheckOutcome::UpToDate => None,
            UpdateCheckOutcome::Available {
                version,
                current_version,
                notes,
                pub_date,
                release_url,
                installable,
                ..
            } => Some(CachedUpdate {
                version: version.clone(),
                current_version: current_version.clone(),
                notes: notes.clone(),
                pub_date: pub_date.clone(),
                release_url: release_url.clone(),
                installable: *installable,
            }),
        };
    }

    Ok(outcome)
}

/// True iff `version` is in the user's skip-list.
async fn is_version_skipped(state: &AppState, version: &str) -> bool {
    use crate::commands::settings::SettingsLoadState;
    let guard = state.settings.read().await;
    match &*guard {
        SettingsLoadState::Loaded(s) => s
            .skipped_update_versions
            .iter()
            .any(|v| v == version),
        _ => false,
    }
}

/// Inner: run an install via the supplied backend, after validating the
/// caller's `version` arg against the cached `Available` payload,
/// rejecting downgrades, and refusing releases that are not installable
/// in place.
pub async fn run_install(
    state: &AppState,
    backend: &dyn InstallBackend,
    version: &str,
) -> Result<(), AppError> {
    state.require_network("update_check").await?;

    // 1. Sanity check the caller-supplied version against the in-memory
    // cached `Available` payload. Defends against UI staleness: if the
    // user kept the Settings panel open through an auto-check cycle and
    // the available version changed, the install button fires with the
    // *old* version arg.
    let cached = {
        let guard = state.updater_state.read().await;
        guard.cached_available.clone()
    };
    let cached = cached.ok_or_else(|| AppError::InvalidArgument {
        message: format!(
            "no update available to install; run update_check_now first (requested {version})"
        ),
    })?;
    if cached.version != version {
        return Err(AppError::InvalidArgument {
            message: format!(
                "install version mismatch: requested {version}, cached available is {}",
                cached.version
            ),
        });
    }

    // 2. Installability gate. A published release without a
    // `latest.json` asset can be announced, but the plugin has nothing
    // to fetch. Refuse with a message that points at the manual path.
    if !cached.installable {
        return Err(AppError::InvalidArgument {
            message: format!(
                "release {version} has no in-app update manifest; \
                 open the release page to download it manually"
            ),
        });
    }

    // 3. Explicit downgrade rejection (defense in depth; the plugin's
    // own version comparator already does this, but a future plugin
    // behaviour change cannot reopen the hole if we re-check here).
    let current = current_app_version();
    if !semver::is_strict_upgrade(current, version) {
        return Err(AppError::DowngradeRejected {
            current: current.to_string(),
            target: version.to_string(),
        });
    }

    // 4. Delegate to the plugin via the backend trait. The plugin
    // performs the download + sha256 verification (when the manifest
    // carries a hash) + minisign verification + atomic .app bundle
    // replacement in a single call.
    backend.download_and_install(version).await?;

    // 5. Clear the cached available payload so a re-render of the
    // indicator + the Settings card doesn't re-offer the same
    // install. The new binary is on disk; the only remaining action
    // is the relaunch, which `update_relaunch` handles.
    {
        let mut guard = state.updater_state.write().await;
        guard.cached_available = None;
        guard.last_outcome = Some(UpdateCheckOutcome::UpToDate);
    }

    Ok(())
}

// ---------- Auto-install (Phase 3) ----------

/// Tauri event emitted for every observable step of a background auto-install.
///
/// The renderer listens once at the app root (`+layout.svelte`) and folds the
/// phases into `updater.autoInstalling` / `installComplete` / `error`, plus a
/// single Activity Journal row on a terminal phase. Emitting from the backend
/// rather than driving the install from the renderer is what makes the feature
/// survive navigation: the user can be in any view (or no view at all) and the
/// install still completes.
///
/// `#[allow(dead_code)]` because the test build never reaches the emitter (see
/// `emit_auto_install`); the constant still documents the contract for the
/// renderer mirror in `updater.svelte.ts`.
#[allow(dead_code)]
pub const AUTO_INSTALL_EVENT: &str = "update:auto-install";

/// Observable phases of a background auto-install.
///
/// `Verified` is a distinct phase from `Installed` because the two happen at
/// different points in the pipeline: the artifact's authenticity is proven at
/// `Verified` (download complete + minisign check passed), and the disk only
/// changes at `Installed` (bundle swap). Collapsing them would hide the only
/// interval in which a cancel can still prevent a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoInstallPhase {
    Started,
    Verified,
    Installed,
    Failed,
    Canceled,
}

/// Wire payload for [`AUTO_INSTALL_EVENT`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoInstallEvent {
    pub phase: AutoInstallPhase,
    pub version: String,
    /// Typed error text, present only on `Failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Emit one [`AutoInstallEvent`] and mirror it into the tracing log.
///
/// The log line is the audit trail required by the Phase 1 compliance gate:
/// phase + version + timestamp, no PII. A failed emit is a warning — losing a
/// notification must never abort or roll back an install.
///
/// `#[allow(dead_code)]` because both this and [`AUTO_INSTALL_EVENT`] are only
/// reachable from `#[cfg(not(test))]` code (`maybe_auto_install` /
/// `PluginBackend::auto`). The test build still covers the wire shape directly
/// via `AutoInstallEvent`, which is what the renderer actually parses.
#[allow(dead_code)]
fn emit_auto_install<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    phase: AutoInstallPhase,
    version: &str,
    message: Option<String>,
) {
    use tauri::Emitter;
    tracing::info!(phase = ?phase, version, "updater: auto-install event");
    let payload = AutoInstallEvent {
        phase,
        version: version.to_string(),
        message,
    };
    if let Err(e) = app.emit(AUTO_INSTALL_EVENT, payload) {
        tracing::warn!("updater: auto-install event emit failed (non-fatal): {e:?}");
    }
}

/// Why an auto-install attempt stopped without installing.
///
/// A skip is never surfaced as an error: every variant here means "the
/// preconditions for an unattended install were not met", which is the correct
/// steady state, not a fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoInstallSkip {
    /// `update_auto_install` is off — never opted in, or consent revoked.
    Disabled,
    /// Offline Mode is on, or settings are corrupt (fail closed).
    Offline,
    /// The version is on the user's skip-list.
    SkippedVersion,
    /// A background install is already in flight (single-flight guard).
    AlreadyRunning,
    /// No cached entry, no in-app manifest, or a downgrade — the cases
    /// `run_install` refuses with a typed gate error.
    NotInstallable,
}

/// Result of one background auto-install attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoInstallResult {
    Installed,
    Canceled,
    Skipped(AutoInstallSkip),
}

/// Evaluate every gate that can block an unattended install for `version`.
///
/// Reads the live settings rather than accepting a snapshot: the user can flip
/// the toggle or Offline Mode at any moment, and consent revocation must win
/// over a flag read milliseconds earlier. `None` means "clear to proceed".
///
/// Kept separate from [`run_auto_install`] so the scheduler can decide whether
/// emitting a `Started` event is even truthful before it does so.
pub async fn auto_install_blocker(state: &AppState, version: &str) -> Option<AutoInstallSkip> {
    use crate::commands::settings::SettingsLoadState;

    let (enabled, paranoid_on) = {
        let guard = state.settings.read().await;
        match &*guard {
            SettingsLoadState::Loaded(s) => (s.update_auto_install, s.paranoid_mode),
            // No settings file yet → defaults, and the default is off.
            SettingsLoadState::FirstLaunch => (false, false),
            // Corrupt → fail closed, exactly like `require_network`.
            SettingsLoadState::Corrupt { .. } => (false, true),
        }
    };
    // Offline Mode (and corrupt-settings fail-closed) is the hard gate and is
    // reported first: when both conditions hold, "offline" is the reason that
    // matters, because it is the one the user must change to proceed.
    if paranoid_on {
        return Some(AutoInstallSkip::Offline);
    }
    if !enabled {
        return Some(AutoInstallSkip::Disabled);
    }
    if is_version_skipped(state, version).await {
        return Some(AutoInstallSkip::SkippedVersion);
    }
    if matches!(
        state.updater_state.read().await.auto_install,
        AutoInstallStatus::Running { .. }
    ) {
        return Some(AutoInstallSkip::AlreadyRunning);
    }
    None
}

/// Inner: run one background auto-install attempt for `version`.
///
/// Gates, in order: [`auto_install_blocker`] (opt-in, Offline Mode, skip-list,
/// single-flight) and then `run_install`'s own defense-in-depth checks (cached
/// version match, installability, downgrade rejection, `require_network`).
///
/// The relaunch is deliberately *not* triggered here: `update_relaunch` stays
/// user-gated, so an unattended install can never restart the app under the
/// user's feet.
pub async fn run_auto_install(
    state: &AppState,
    backend: &dyn InstallBackend,
    version: &str,
) -> Result<AutoInstallResult, AppError> {
    if let Some(skip) = auto_install_blocker(state, version).await {
        return Ok(AutoInstallResult::Skipped(skip));
    }
    // Claim the slot atomically. `auto_install_blocker` already reports
    // `AlreadyRunning`, but its read is separated from this write by an
    // `.await`, so a manual install can slip in between. Only the single
    // write-lock section in `claim_install_slot` makes single-flight real.
    if !claim_install_slot(state, version).await {
        return Ok(AutoInstallResult::Skipped(AutoInstallSkip::AlreadyRunning));
    }

    let outcome = match run_install(state, backend, version).await {
        Ok(()) => {
            set_auto_install_status(state, AutoInstallStatus::Installed { version: version.to_string() })
                .await;
            AutoInstallResult::Installed
        }
        Err(AppError::InstallCanceled) => {
            set_auto_install_status(state, AutoInstallStatus::Canceled { version: version.to_string() })
                .await;
            AutoInstallResult::Canceled
        }
        // Gate refusals are skips: nothing actionable happened, so the user
        // gets no error toast for them.
        Err(AppError::ParanoidModeBlocked { .. }) => {
            set_auto_install_status(state, AutoInstallStatus::Idle).await;
            AutoInstallResult::Skipped(AutoInstallSkip::Offline)
        }
        Err(AppError::InvalidArgument { .. }) | Err(AppError::DowngradeRejected { .. }) => {
            set_auto_install_status(state, AutoInstallStatus::Idle).await;
            AutoInstallResult::Skipped(AutoInstallSkip::NotInstallable)
        }
        Err(e) => {
            set_auto_install_status(
                state,
                AutoInstallStatus::Failed {
                    version: version.to_string(),
                    message: e.to_string(),
                },
            )
            .await;
            return Err(e);
        }
    };
    Ok(outcome)
}

/// Inner: run one **manual** install under the shared single-flight slot.
///
/// Extracted from the `update_install` command so the guard is compiled into
/// the test build. F-02 lived in `#[cfg(not(test))]` code and was therefore
/// invisible to the suite; keeping the claim/release here means a regression
/// fails a test instead of shipping.
///
/// `run_install` records no terminal auto-install status, so every exit must
/// free the slot explicitly — otherwise a single failed install would make
/// every later install report "already running".
pub async fn run_manual_install(
    state: &AppState,
    backend: &dyn InstallBackend,
    version: &str,
) -> Result<(), AppError> {
    if !claim_install_slot(state, version).await {
        return Err(AppError::InstallInProgress);
    }
    let result = run_install(state, backend, version).await;
    release_install_slot(state).await;
    result
}

async fn set_auto_install_status(state: &AppState, status: AutoInstallStatus) {
    state.updater_state.write().await.auto_install = status;
}

/// Atomically claim the one install slot for `version`.
///
/// Returns `false` when an install is already running — background *or* manual.
/// The check and the state write happen inside a single write-lock critical
/// section on purpose: a check-then-set separated by an `.await` lets two
/// callers both pass the gate before either records `Running`, and the bundle
/// swap is not re-entrant.
///
/// `Running` is the guard *and* the UI status, so a manual install is visible as
/// a running install for its duration and the background path cannot start
/// underneath it.
async fn claim_install_slot(state: &AppState, version: &str) -> bool {
    let mut guard = state.updater_state.write().await;
    if matches!(guard.auto_install, AutoInstallStatus::Running { .. }) {
        return false;
    }
    guard.auto_install = AutoInstallStatus::Running {
        version: version.to_string(),
    };
    true
}

/// Free the slot claimed by [`claim_install_slot`] when the attempt ended
/// without recording a terminal status of its own.
///
/// Scoped to `Running` so it can never clobber a terminal status (`Installed`,
/// `Failed`, `Canceled`) that the caller already set — those already free the
/// slot, and overwriting them would erase the relaunch prompt or the error.
async fn release_install_slot(state: &AppState) {
    let mut guard = state.updater_state.write().await;
    if matches!(guard.auto_install, AutoInstallStatus::Running { .. }) {
        guard.auto_install = AutoInstallStatus::Idle;
    }
}

/// The two consents that gate an unattended install, sampled before and after a
/// settings write.
///
/// A single struct rather than loose booleans because the interesting question
/// is always "what changed?", and a four-bool signature invites call sites that
/// pass them in the wrong order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoInstallConsent {
    pub enabled: bool,
    pub offline: bool,
}

/// Consent-revocation hook, called by `settings_set` / `settings_reset` after a
/// successful write.
///
/// Two transitions must abort an in-flight background install before the bundle
/// swap, because waiting for the next 24h wake would let an install land that
/// the user just forbade:
///
/// 1. **Auto-install turned off** — explicit withdrawal of the opt-in.
/// 2. **Offline Mode turned on** — the user asserted "no outbound activity";
///    honouring that only at the *start* of the next attempt would still swap
///    the bundle under them mid-download.
///
/// Note the cancel point is after download-verify, so it stops the *install*,
/// not the already-in-flight bytes. That is the honest contract: nothing on
/// disk changes once the flag is set.
///
/// Returns `true` when a cancellation was requested. The return value exists so
/// the transition is unit-testable without the Tauri `State` wrapper.
pub async fn handle_auto_install_consent_change(
    state: &AppState,
    before: AutoInstallConsent,
    after: AutoInstallConsent,
) -> bool {
    let revoked = before.enabled && !after.enabled;
    let went_offline = !before.offline && after.offline;
    if revoked || went_offline {
        state.updater_state.read().await.auto_install_cancel.request();
        return true;
    }
    false
}

// ---------- IPC commands ----------

/// Run a manual update check. Frontend-callable via the "Check for
/// updates now" button in Settings → Network → Updates.
///
/// Returns `UpdateCheckOutcome` on success or `AppError` on failure
/// (including `ParanoidModeBlocked` when Offline Mode is on, surfaced
/// as the typed error rather than a fourth enum variant so the toast
/// channel stays uniform with every other gated call).
#[tauri::command]
pub async fn update_check_now(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdateCheckOutcome, AppError> {
    #[cfg(test)]
    {
        let _ = (app, state);
        // Tests never go through this path; they exercise `run_check`
        // directly via the trait. The `app` + `state` parameters are
        // retained so the IPC signature stays uniform with the
        // production build.
        Err(AppError::Internal {
            message: "update_check_now is not callable in tests".into(),
        })
    }
    #[cfg(not(test))]
    {
        // The release source is built per call: the Keychain token can
        // change between checks (sign-in / sign-out) and building it
        // here keeps the `run_check` signature free of Tauri types.
        let source = build_release_source()?;
        run_check(&state, &source).await
    }
}

/// Install the most recently-cached available update. Frontend passes
/// the `version` it expects to install so a stale UI doesn't trigger
/// an install of the wrong binary.
#[tauri::command]
pub async fn update_install(
    version: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    #[cfg(test)]
    {
        let _ = (app, version, state);
        Err(AppError::Internal {
            message: "update_install is not callable in tests".into(),
        })
    }
    #[cfg(not(test))]
    {
        let backend = PluginBackend::new(app);
        // The claim lives in `run_manual_install` so it is covered by tests: the
        // renderer also refuses a second install, but that is a boolean set by
        // an async event after a round trip — a UI affordance, not a safety
        // mechanism. This is what stops a manual install and a background
        // install of the same version from both swapping the bundle.
        run_manual_install(&state, &backend, &version).await
    }
}

/// Inner: record a skip against the supplied AppState. Extracted so
/// unit tests can exercise the Corrupt-settings safety branch without
/// going through the Tauri State wrapper.
///
/// ⚠️ Corrupt-settings safety: when settings are Corrupt, the app is
/// in fail-closed paranoid-on state (require_network denies all
/// outbound). Writing `Settings::default()` here would silently revoke
/// that lockdown (paranoid_mode = false in defaults). Refuse the skip
/// instead — the frontend's optimistic UI clear of `available` still
/// hides the indicator for this session; the user must reset settings
/// before a persisted skip is possible.
pub async fn run_skip(state: &AppState, version: &str) -> Result<(), AppError> {
    use crate::commands::settings::{persist, SettingsLoadState};

    // Validate the version arg — defensive against UI passing garbage.
    if version.trim().is_empty() || version.len() > 64 {
        return Err(AppError::InvalidArgument {
            message: format!("invalid version for skip: {version:?}"),
        });
    }

    // 1) Mutate the settings struct (push_skipped_version handles cap +
    //    dedupe internally). Take a snapshot to hand to persist().
    let updated_settings = {
        let guard = state.settings.read().await;
        match &*guard {
            SettingsLoadState::Loaded(s) => {
                let mut s = s.clone();
                s.push_skipped_version(version.to_string());
                s
            }
            SettingsLoadState::FirstLaunch => {
                // No settings file yet — defaults are correct (paranoid
                // OFF matches "user has never configured anything"),
                // and materializing the file with the skip recorded is
                // the right next step.
                let mut s = crate::commands::settings::Settings::default();
                s.push_skipped_version(version.to_string());
                s
            }
            SettingsLoadState::Corrupt { message } => {
                return Err(AppError::Internal {
                    message: format!(
                        "cannot record update skip while settings file is unreadable \
                         ({message}); reset settings from Settings → Network first"
                    ),
                });
            }
        }
    };
    let clamped = persist(&state.app_data_dir, updated_settings).await?;
    {
        let mut guard = state.settings.write().await;
        *guard = SettingsLoadState::Loaded(clamped);
    }

    // 2) Clear the cached "available" entry when it matches the skipped
    //    version, so subsequent update_check_now() responses + the
    //    title-bar indicator state are coherent. The frontend already
    //    flips its own `available = null` optimistically; this keeps
    //    the backend's view in sync.
    {
        let mut guard = state.updater_state.write().await;
        let should_clear = guard
            .cached_available
            .as_ref()
            .is_some_and(|cached| cached.version == version);
        if should_clear {
            guard.cached_available = None;
        }
    }

    Ok(())
}

/// Skip a specific update version. Pushes onto the FIFO-capped skip
/// list in settings (10 entries max, oldest evicted, dedup-and-move-to-
/// tail per `Settings::push_skipped_version`) and clears the cached
/// "available" entry when its version matches — so the title-bar
/// indicator disappears immediately on the next reactive render.
///
/// No `require_network` gate: skipping an update is a purely local
/// state mutation that records a user preference. It doesn't reach
/// the network. Sane to allow even when Offline Mode is on (the user
/// might be reviewing accumulated update notices while offline).
#[tauri::command]
pub async fn update_skip(
    version: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    run_skip(&state, &version).await
}

/// Relaunch the running app process so a freshly-installed update
/// becomes the active binary. The plugin's macOS install path replaces
/// the .app bundle but does not auto-restart the running process; this
/// command bridges that gap.
///
/// The restart itself is fired on a short delay so the IPC response
/// arrives at the renderer before the process dies. `tauri::AppHandle::
/// restart()` is `-> !` — calling it directly from the command body
/// would tear the IPC socket down mid-response.
///
/// No `require_network` gate: this is a purely local process action.
#[tauri::command]
pub async fn update_relaunch(app: tauri::AppHandle) -> Result<(), AppError> {
    #[cfg(test)]
    {
        let _ = app;
        Err(AppError::Internal {
            message: "update_relaunch is not callable in tests".into(),
        })
    }
    #[cfg(not(test))]
    {
        tauri::async_runtime::spawn(async move {
            // 50ms is enough for the JSON IPC response to make it to
            // the renderer before the process restarts.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            app.restart();
        });
        Ok(())
    }
}

/// Cancel an in-flight background auto-install.
///
/// Cooperative by design: the flag is consulted between download-verify and the
/// bundle swap, so a cancel that lands after the swap is a harmless no-op (the
/// update is installed and the UI shows the relaunch prompt instead).
///
/// No `require_network` gate — this is a purely local state mutation, and it
/// must keep working when the user's way of stopping the install *is* turning
/// Offline Mode on.
#[tauri::command]
pub async fn update_auto_install_cancel(state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .updater_state
        .read()
        .await
        .auto_install_cancel
        .request();
    Ok(())
}

// ---------- Auto-check scheduler ----------

/// Minimum wall-clock interval between auto-checks, regardless of how
/// often the app is restarted. 24h matches Sparkle's default + macOS
/// App Store cadence (see Phase 15 plan §Resolved Decision #3).
pub const AUTO_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Delay before the **first** auto-check after launch.
///
/// Not zero — the check is an outbound GitHub API call and the app should be
/// interactive first. Not one full interval either: `last_checked_at` is
/// memory-only, so every launch restarted the 24h sleep and a user who restarts
/// the app more often than once a day never got a check at all. Since this
/// scheduler is the only trigger for an unattended install, that made the whole
/// feature unreachable. One check per launch is the Chrome / VS Code / Sparkle
/// norm and costs one unauthenticated request.
pub const AUTO_CHECK_STARTUP_DELAY: std::time::Duration = std::time::Duration::from_secs(60);

/// Re-poll interval while auto-check is inactive: not opted in, or Offline Mode
/// on (including corrupt settings, which fail closed).
///
/// The scheduler cannot sleep a full day while the toggle is off — a user who
/// turns it on mid-session would wait a day for the first check, which is the
/// dead-control failure this feature exists to remove. Five minutes is a
/// deliberate compromise: three uncontended lock reads and no network, in
/// exchange for a toggle flip taking effect without a relaunch.
pub const AUTO_CHECK_IDLE_POLL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Upper bound of the startup jitter, in seconds.
pub const AUTO_CHECK_STARTUP_JITTER_SECS: u64 = 30;

/// First sleep of the scheduler: [`AUTO_CHECK_STARTUP_DELAY`] plus up to
/// [`AUTO_CHECK_STARTUP_JITTER_SECS`] of jitter.
///
/// Jitter spreads a fleet of clients that launch together (after an OS update,
/// say) so they do not hit the GitHub API in one burst. Derived from the wall
/// clock rather than an RNG: no new dependency, and the value is reproducible
/// for a given `now`, so the bound is unit-testable.
pub fn startup_delay(now: i64) -> std::time::Duration {
    let jitter = now.unsigned_abs() % AUTO_CHECK_STARTUP_JITTER_SECS;
    AUTO_CHECK_STARTUP_DELAY + std::time::Duration::from_secs(jitter)
}

/// Backoff steps when an auto-check fails. Order is 1h → 6h → 24h;
/// after the third failure the next attempt waits a full 24h window.
pub const AUTO_CHECK_BACKOFF: &[std::time::Duration] = &[
    std::time::Duration::from_secs(60 * 60),
    std::time::Duration::from_secs(6 * 60 * 60),
    std::time::Duration::from_secs(24 * 60 * 60),
];

/// Delay after a successful check: the canonical 24h cadence.
pub fn sleep_after_success() -> std::time::Duration {
    AUTO_CHECK_INTERVAL
}

/// Delay after a failed check: the backoff step for `attempt` (0-based),
/// saturating at the last step so repeated failures settle at 24h.
pub fn sleep_after_failure(attempt: usize) -> std::time::Duration {
    AUTO_CHECK_BACKOFF
        .get(attempt)
        .copied()
        .unwrap_or(AUTO_CHECK_INTERVAL)
}

/// Next backoff index, saturating at the last step.
pub fn next_backoff_index(attempt: usize) -> usize {
    (attempt + 1).min(AUTO_CHECK_BACKOFF.len() - 1)
}

/// Decide the scheduler's next move after a wake: `None` means "run a check
/// now", `Some(delay)` means "sleep `delay`, then ask again".
///
/// Pure — extracted so the cadence is unit-testable without an `AppState`, a
/// network, or a filesystem. This is the **only** definition of "should the
/// scheduler fire": the loop and the tests both call it, so the tested gate and
/// the shipped gate cannot drift.
///
/// Three cases:
/// - **Inactive** (not opted in, Offline Mode on, corrupt settings) →
///   [`AUTO_CHECK_IDLE_POLL`], so a toggle flip is noticed without a relaunch.
/// - **Throttled** (opted in, but the 24h floor since the last check has not
///   elapsed) → the *remaining* floor. A fresh interval here would silently
///   stretch the cadence to 48h whenever the floor expires mid-sleep.
/// - **Due** → `None`.
pub fn next_sleep(
    auto_check_enabled: bool,
    paranoid_mode: bool,
    last_checked_at: Option<i64>,
    now: i64,
) -> Option<std::time::Duration> {
    if !auto_check_enabled || paranoid_mode {
        return Some(AUTO_CHECK_IDLE_POLL);
    }
    match last_checked_at {
        None => None,
        Some(prev) => {
            let elapsed = now.saturating_sub(prev).max(0) as u64;
            let floor = AUTO_CHECK_INTERVAL.as_secs();
            if elapsed >= floor {
                None
            } else {
                Some(std::time::Duration::from_secs(floor - elapsed))
            }
        }
    }
}

/// Spawn the auto-check scheduler as a tokio background task. Called
/// once at app startup from `state::initialize`. The task:
///
/// 1. Sleeps [`AUTO_CHECK_STARTUP_DELAY`] before the first wake, then
///    [`AUTO_CHECK_INTERVAL`] between successful checks.
/// 2. Reads the live settings (re-reads each cycle so a user who
///    toggles auto-check off mid-run is honoured on the next wake).
/// 3. Asks [`next_sleep`] whether to check now or wait; an inactive
///    scheduler re-polls at [`AUTO_CHECK_IDLE_POLL`] so flipping the
///    toggle on takes effect without a relaunch.
/// 4. Runs `run_check`, hands an available version to
///    `maybe_auto_install`, and backs off on failure.
/// 5. Loops forever.
pub fn spawn_auto_check_scheduler<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    #[cfg(not(test))]
    {
        use tauri::Manager;
        tauri::async_runtime::spawn(async move {
            // First wake shortly after launch (see AUTO_CHECK_STARTUP_DELAY):
            // deferring it by a full interval meant the process was usually
            // gone before the first check, so nothing was ever discovered and
            // nothing could ever be auto-installed.
            let mut sleep_for = startup_delay(chrono::Utc::now().timestamp());
            let mut backoff_idx = 0usize;
            loop {
                tokio::time::sleep(sleep_for).await;

                let state: tauri::State<AppState> = app.state();
                let (auto_on, paranoid_on, last_checked_at) = read_scheduler_inputs(&state).await;
                let now = chrono::Utc::now().timestamp();

                if let Some(wait) = next_sleep(auto_on, paranoid_on, last_checked_at, now) {
                    // Either inactive (poll soon so a toggle flip is noticed)
                    // or throttled by the 24h floor (wait exactly the
                    // remainder). No check runs, so no backoff state applies.
                    sleep_for = wait;
                    backoff_idx = 0;
                    continue;
                }

                // Build the release source per wake: the Keychain token
                // can change between cycles, and a stale source would
                // keep using (or keep lacking) auth.
                let source = match build_release_source() {
                    Ok(source) => source,
                    Err(e) => {
                        tracing::warn!("updater: auto-check source build failed (non-fatal): {e:?}");
                        sleep_for = sleep_after_failure(backoff_idx);
                        backoff_idx = next_backoff_index(backoff_idx);
                        continue;
                    }
                };
                match run_check(&state, &source).await {
                    Ok(outcome) => {
                        sleep_for = sleep_after_success();
                        backoff_idx = 0;
                        // Phase 3 — hand the discovered update to the
                        // auto-install path. It re-reads settings and owns
                        // every gate, so a revoked opt-in is honoured even
                        // though the check above just succeeded.
                        maybe_auto_install(&app, &state, &outcome).await;
                    }
                    Err(e) => {
                        tracing::warn!("updater: auto-check failed (non-fatal): {e:?}");
                        sleep_for = sleep_after_failure(backoff_idx);
                        backoff_idx = next_backoff_index(backoff_idx);
                    }
                }
            }
        });
    }
    #[cfg(test)]
    {
        // Under cfg(test) we don't spawn the real scheduler — its loop
        // body references the PluginBackend which is itself cfg-gated
        // out so it can't be instantiated. Tests exercise the pure
        // `next_sleep` cadence function directly and the IPC
        // commands through the trait-injected mock backend.
        let _ = app;
    }
}

/// Read the settings the scheduler needs in a single guard acquisition.
/// Returns `(should_run_checks, paranoid_mode, last_checked_at)`.
///
/// `should_run_checks` is `update_auto_check || update_auto_install`. Auto-install
/// cannot exist without discovery, so gating discovery on the *other* toggle
/// would leave the auto-install toggle permanently inert — a dead control,
/// which is the exact defect this feature set out to fix.
#[cfg(not(test))]
async fn read_scheduler_inputs(state: &AppState) -> (bool, bool, Option<i64>) {
    use crate::commands::settings::SettingsLoadState;
    let (should_check, paranoid_on) = {
        let guard = state.settings.read().await;
        match &*guard {
            SettingsLoadState::Loaded(s) => (
                s.update_auto_check || s.update_auto_install,
                s.paranoid_mode,
            ),
            SettingsLoadState::FirstLaunch => (false, false),
            // Corrupt: deny outbound. The require_network gate inside
            // run_check would also catch this, but short-circuiting here
            // saves us a wakeup + a network attempt.
            SettingsLoadState::Corrupt { .. } => (false, true),
        }
    };
    let last_checked_at = {
        let guard = state.updater_state.read().await;
        guard.last_checked_at
    };
    (should_check, paranoid_on, last_checked_at)
}

/// Follow-up to a successful auto-check: install the discovered update in the
/// background when the user opted in.
///
/// Owns the event emission and nothing else — every gate lives in
/// [`auto_install_blocker`] / [`run_auto_install`] so there is exactly one
/// definition of "may this install happen". Emitting `Started` only after the
/// gates pass keeps the event stream truthful for the pre-flight case; every
/// exit *after* `Started` emits a terminal phase, because the renderer clears
/// its progress row only on a terminal phase.
#[cfg(not(test))]
async fn maybe_auto_install<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    outcome: &UpdateCheckOutcome,
) {
    let UpdateCheckOutcome::Available {
        version,
        installable,
        skipped,
        ..
    } = outcome
    else {
        return;
    };
    // A skipped version is never auto-installed (the user dismissed it), and a
    // release with no in-app manifest has nothing to install in place.
    if !installable || *skipped {
        return;
    }

    // Arm a fresh cancellation window first: a cancel requested during the
    // previous cycle must not abort this one. Order matters — resetting *after*
    // the gate check would erase a revocation that landed between the check and
    // the install, and the install would proceed against the user's wish.
    let cancel = state.updater_state.read().await.auto_install_cancel.clone();
    cancel.reset();

    if auto_install_blocker(state, version).await.is_some() {
        return;
    }

    emit_auto_install(app, AutoInstallPhase::Started, version, None);

    // The `Verified` milestone is only observable from inside the plugin
    // pipeline, so the backend reports it back through this closure. It fires
    // only once the cancel checkpoint has been passed.
    let verified_app = app.clone();
    let verified_version = version.clone();
    let backend = PluginBackend::auto(app.clone(), cancel, move || {
        emit_auto_install(
            &verified_app,
            AutoInstallPhase::Verified,
            &verified_version,
            None,
        );
    });

    match run_auto_install(state, &backend, version).await {
        Ok(AutoInstallResult::Installed) => {
            emit_auto_install(app, AutoInstallPhase::Installed, version, None);
        }
        Ok(AutoInstallResult::Canceled) => {
            emit_auto_install(app, AutoInstallPhase::Canceled, version, None);
        }
        // Preconditions changed between the check above and the attempt:
        // Offline Mode flipped on, consent revoked, or a manual install claimed
        // the slot. Reported as `canceled` because that is what happened — the
        // attempt ended without installing — and because a `Started` with no
        // terminal phase would leave the renderer's progress row stuck for the
        // rest of the session, with a Cancel button that polls nothing.
        Ok(AutoInstallResult::Skipped(_)) => {
            emit_auto_install(app, AutoInstallPhase::Canceled, version, None);
        }
        Err(e) => emit_auto_install(app, AutoInstallPhase::Failed, version, Some(e.to_string())),
    }
}

// Re-export so `AppState::build()` can construct the wrapped state slot.
pub fn empty_state() -> Arc<RwLock<UpdaterState>> {
    Arc::new(RwLock::new(UpdaterState::default()))
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::settings::{Settings, SettingsLoadState};
    use std::sync::Mutex;

    /// In-memory mock that satisfies both halves of the updater surface:
    /// the release channel ([`ReleaseSource`]) and the installer
    /// ([`InstallBackend`]). One struct keeps the existing test bodies
    /// readable — a check and the install that follows it share the same
    /// canned wiring.
    struct MockBackend {
        release_result: Mutex<Result<Option<ReleaseInfo>, AppError>>,
        install_result: Mutex<Result<(), AppError>>,
        check_calls: Mutex<u32>,
        install_calls: Mutex<u32>,
    }

    impl MockBackend {
        fn returning(release: Result<Option<ReleaseInfo>, AppError>) -> Self {
            Self {
                release_result: Mutex::new(release),
                install_result: Mutex::new(Ok(())),
                check_calls: Mutex::new(0),
                install_calls: Mutex::new(0),
            }
        }

        fn install_returning(
            release: Result<Option<ReleaseInfo>, AppError>,
            install: Result<(), AppError>,
        ) -> Self {
            Self {
                release_result: Mutex::new(release),
                install_result: Mutex::new(install),
                check_calls: Mutex::new(0),
                install_calls: Mutex::new(0),
            }
        }
    }

    /// Build a published-release fixture. Defaults to installable so the
    /// happy paths don't need to spell every field.
    fn release(version: &str) -> ReleaseInfo {
        ReleaseInfo {
            version: version.into(),
            tag: format!("v{version}"),
            name: Some(format!("Agency Agents v{version}")),
            notes: None,
            published_at: None,
            html_url: format!(
                "https://github.com/deuriib/agency-agents-app/releases/tag/v{version}"
            ),
            installable: true,
        }
    }

    #[async_trait::async_trait]
    impl ReleaseSource for MockBackend {
        async fn latest(&self) -> Result<Option<ReleaseInfo>, AppError> {
            *self.check_calls.lock().unwrap() += 1;
            self.release_result.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl InstallBackend for MockBackend {
        async fn download_and_install(&self, _version: &str) -> Result<(), AppError> {
            *self.install_calls.lock().unwrap() += 1;
            self.install_result.lock().unwrap().clone()
        }
    }

    async fn build_state_with(slot: SettingsLoadState) -> AppState {
        let state = AppState::build().expect("AppState::build");
        {
            let mut guard = state.settings.write().await;
            *guard = slot;
        }
        state
    }

    /// Phase 15 §Tests #1 — happy path: plugin returns "no update";
    /// command returns `UpToDate`.
    #[tokio::test]
    async fn check_now_returns_up_to_date_when_plugin_returns_none() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::returning(Ok(None));
        let outcome = run_check(&state, &backend).await.expect("check");
        assert_eq!(outcome, UpdateCheckOutcome::UpToDate);

        // last_checked_at must be populated regardless of outcome so
        // the scheduler honours the 24h floor on UpToDate too.
        let guard = state.updater_state.read().await;
        assert!(guard.last_checked_at.is_some());
        assert!(guard.cached_available.is_none());
    }

    /// Phase 15 §Tests #2 — available path: the release channel publishes
    /// a newer version, command returns `Available { ... }` with the
    /// right fields plus the release metadata the UI needs.
    #[tokio::test]
    async fn check_now_returns_available_when_release_is_newer() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::returning(Ok(Some(ReleaseInfo {
            notes: Some("hotfix".into()),
            published_at: Some("2026-05-24T00:00:00Z".into()),
            ..release("9.9.9")
        })));
        let outcome = run_check(&state, &backend).await.expect("check");
        match outcome {
            UpdateCheckOutcome::Available {
                version,
                current_version,
                notes,
                release_url,
                installable,
                skipped,
                ..
            } => {
                assert_eq!(version, "9.9.9");
                assert_eq!(current_version, current_app_version());
                assert_eq!(notes.as_deref(), Some("hotfix"));
                assert_eq!(
                    release_url,
                    "https://github.com/deuriib/agency-agents-app/releases/tag/v9.9.9"
                );
                assert!(installable);
                assert!(!skipped, "fresh version must not be marked skipped");
            }
            other => panic!("expected Available, got {other:?}"),
        }

        // Cached available payload available for install validation.
        let guard = state.updater_state.read().await;
        let cached = guard.cached_available.clone().expect("cached");
        assert_eq!(cached.version, "9.9.9");
        assert!(cached.installable);
    }

    /// A release whose tag matches the running version is not an update.
    /// Re-publishing the same version (asset rebuild) must never surface
    /// an install affordance.
    #[tokio::test]
    async fn check_now_returns_up_to_date_for_same_version() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::returning(Ok(Some(release(current_app_version()))));
        let outcome = run_check(&state, &backend).await.expect("check");
        assert_eq!(outcome, UpdateCheckOutcome::UpToDate);

        let guard = state.updater_state.read().await;
        assert!(guard.cached_available.is_none());
        assert!(guard.last_checked_at.is_some());
    }

    /// An older published release (rollback scenario) is not an update.
    #[tokio::test]
    async fn check_now_returns_up_to_date_for_older_version() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::returning(Ok(Some(release("0.0.1"))));
        let outcome = run_check(&state, &backend).await.expect("check");
        assert_eq!(outcome, UpdateCheckOutcome::UpToDate);
    }

    /// A release without a `latest.json` asset is announced but flagged
    /// not installable, so the UI can offer "View release" instead.
    #[tokio::test]
    async fn check_now_marks_release_without_manifest_not_installable() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::returning(Ok(Some(ReleaseInfo {
            installable: false,
            ..release("9.9.9")
        })));
        let outcome = run_check(&state, &backend).await.expect("check");
        match outcome {
            UpdateCheckOutcome::Available { installable, .. } => assert!(!installable),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    /// Available + version is in the skip-list → `skipped: true`.
    #[tokio::test]
    async fn check_now_marks_skipped_when_version_is_in_skip_list() {
        let s = Settings {
            skipped_update_versions: vec!["9.9.9".into()],
            ..Settings::default()
        };
        let state = build_state_with(SettingsLoadState::Loaded(s)).await;
        let backend = MockBackend::returning(Ok(Some(release("9.9.9"))));
        let outcome = run_check(&state, &backend).await.expect("check");
        match outcome {
            UpdateCheckOutcome::Available { skipped, .. } => assert!(skipped),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    /// Phase 15 §Tests #3 — blocked by Paranoid Mode: returns
    /// `ParanoidModeBlocked { feature: "update_check" }`.
    #[tokio::test]
    async fn check_now_blocked_by_paranoid_mode() {
        let s = Settings {
            paranoid_mode: true,
            ..Settings::default()
        };
        let state = build_state_with(SettingsLoadState::Loaded(s)).await;
        let backend = MockBackend::returning(Ok(None));
        let r = run_check(&state, &backend).await;
        match r {
            Err(AppError::ParanoidModeBlocked { feature }) => {
                assert_eq!(feature, "update_check");
            }
            other => panic!("expected ParanoidModeBlocked, got {other:?}"),
        }

        // Backend must NOT have been called — gate runs before the trait call.
        assert_eq!(*backend.check_calls.lock().unwrap(), 0);
    }

    /// Phase 15 §Tests #4 — install rejects a stale version arg.
    #[tokio::test]
    async fn install_rejects_stale_version_arg() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        // Seed the cache with an Available 9.9.9.
        let backend = MockBackend::returning(Ok(Some(release("9.9.9"))));
        run_check(&state, &backend).await.expect("check");

        // UI requests install of an OLDER version than the cache (stale UI).
        let r = run_install(&state, &backend, "0.3.0").await;
        match r {
            Err(AppError::InvalidArgument { message }) => {
                assert!(message.contains("mismatch"), "got: {message}");
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    /// Install without a preceding check → InvalidArgument (no cache).
    #[tokio::test]
    async fn install_without_cache_returns_invalid_argument() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::returning(Ok(None));
        let r = run_install(&state, &backend, "9.9.9").await;
        match r {
            Err(AppError::InvalidArgument { message }) => {
                assert!(message.contains("no update available"), "got: {message}");
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    /// Install of a same-or-older version → DowngradeRejected.
    ///
    /// The check path can never produce this cache state (it only caches
    /// strictly-newer releases), so the cache is seeded directly. That is
    /// exactly the scenario the gate defends against: an in-memory cache
    /// written by an earlier build whose running version was higher.
    #[tokio::test]
    async fn install_rejects_downgrade() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let current = current_app_version().to_string();
        {
            let mut guard = state.updater_state.write().await;
            guard.cached_available = Some(CachedUpdate {
                version: current.clone(),
                current_version: current.clone(),
                notes: None,
                pub_date: None,
                release_url: format!(
                    "https://github.com/deuriib/agency-agents-app/releases/tag/v{current}"
                ),
                installable: true,
            });
        }

        let backend = MockBackend::returning(Ok(None));
        let r = run_install(&state, &backend, &current).await;
        match r {
            Err(AppError::DowngradeRejected { current: c, target: t }) => {
                assert_eq!(c, current);
                assert_eq!(t, current);
            }
            other => panic!("expected DowngradeRejected, got {other:?}"),
        }
    }

    /// A release that exists but ships no `latest.json` cannot be
    /// installed in place; the command refuses before touching the plugin.
    #[tokio::test]
    async fn install_rejects_non_installable_release() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::install_returning(
            Ok(Some(ReleaseInfo {
                installable: false,
                ..release("9.9.9")
            })),
            Ok(()),
        );
        run_check(&state, &backend).await.expect("check");

        let r = run_install(&state, &backend, "9.9.9").await;
        match r {
            Err(AppError::InvalidArgument { message }) => {
                assert!(message.contains("no in-app update manifest"), "got: {message}");
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    /// Phase 15 §Tests #8 — signature verification failure surfaces
    /// as the typed `SignatureVerificationFailed` error.
    #[tokio::test]
    async fn install_surfaces_signature_verification_failed() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::install_returning(
            Ok(Some(release("9.9.9"))),
            Err(AppError::SignatureVerificationFailed {
                message: "minisign rejected".into(),
            }),
        );
        run_check(&state, &backend).await.expect("check");

        let r = run_install(&state, &backend, "9.9.9").await;
        match r {
            Err(AppError::SignatureVerificationFailed { message }) => {
                assert!(message.contains("minisign"), "got: {message}");
            }
            other => panic!("expected SignatureVerificationFailed, got {other:?}"),
        }
    }

    /// Phase 15 §Tests #9 (BONUS) — sha256 mismatch surfaces as the
    /// typed `HashMismatch` error before signature verification.
    #[tokio::test]
    async fn install_surfaces_hash_mismatch() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::install_returning(
            Ok(Some(release("9.9.9"))),
            Err(AppError::HashMismatch {
                expected: "deadbeef".into(),
                actual: "cafef00d".into(),
            }),
        );
        run_check(&state, &backend).await.expect("check");

        let r = run_install(&state, &backend, "9.9.9").await;
        match r {
            Err(AppError::HashMismatch { expected, actual }) => {
                assert_eq!(expected, "deadbeef");
                assert_eq!(actual, "cafef00d");
            }
            other => panic!("expected HashMismatch, got {other:?}"),
        }
    }

    /// Phase 15 fix-up — `run_skip` MUST NOT persist `Settings::default()`
    /// when the in-memory state is Corrupt. Doing so silently revokes the
    /// paranoid-on lockdown that Corrupt settings imply. Refusal with a
    /// typed error is the correct behaviour.
    #[tokio::test]
    async fn skip_refuses_on_corrupt_settings() {
        let state = build_state_with(SettingsLoadState::Corrupt {
            message: "synthetic test corruption".into(),
        })
        .await;

        let r = run_skip(&state, "9.9.9").await;
        match r {
            Err(AppError::Internal { message }) => {
                assert!(
                    message.contains("unreadable"),
                    "expected unreadable message, got: {message}"
                );
                assert!(
                    message.contains("reset"),
                    "expected reset guidance, got: {message}"
                );
            }
            other => panic!("expected Internal error refusing skip, got {other:?}"),
        }

        // Settings must still be Corrupt — refused skip must NOT have
        // overwritten the in-memory state.
        let guard = state.settings.read().await;
        match &*guard {
            SettingsLoadState::Corrupt { .. } => {} // expected
            other => panic!("settings state was mutated by refused skip: {other:?}"),
        }
    }

    /// Phase 15 fix-up — invalid version argument rejected with
    /// InvalidArgument, regardless of settings state.
    #[tokio::test]
    async fn skip_rejects_empty_version() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let r = run_skip(&state, "").await;
        match r {
            Err(AppError::InvalidArgument { .. }) => {}
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    /// Phase 15 fix-up — `run_install` clears `cached_available` after a
    /// successful install so the indicator + Settings card don't
    /// re-offer the same install on the next render.
    #[tokio::test]
    async fn install_clears_cached_available_on_success() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        let backend = MockBackend::install_returning(
            Ok(Some(release("9.9.9"))),
            Ok(()),
        );
        run_check(&state, &backend).await.expect("check");
        // Sanity: cache populated by the check.
        {
            let guard = state.updater_state.read().await;
            assert!(guard.cached_available.is_some());
        }

        run_install(&state, &backend, "9.9.9").await.expect("install");

        let guard = state.updater_state.read().await;
        assert!(
            guard.cached_available.is_none(),
            "cached_available must be cleared after successful install"
        );
        assert_eq!(guard.last_outcome, Some(UpdateCheckOutcome::UpToDate));
    }

    /// Install blocked by paranoid mode → ParanoidModeBlocked.
    #[tokio::test]
    async fn install_blocked_by_paranoid_mode() {
        let s = Settings {
            paranoid_mode: true,
            ..Settings::default()
        };
        let state = build_state_with(SettingsLoadState::Loaded(s)).await;
        let backend = MockBackend::returning(Ok(None));
        let r = run_install(&state, &backend, "9.9.9").await;
        match r {
            Err(AppError::ParanoidModeBlocked { feature }) => {
                assert_eq!(feature, "update_check");
            }
            other => panic!("expected ParanoidModeBlocked, got {other:?}"),
        }
    }

    // ---------- Scheduler ----------

    /// Phase 15 §Tests #6 — scheduler honours the 24h floor.
    /// Repeated wakes within the window never approve a fresh check.
    #[test]
    fn scheduler_honors_24h_floor() {
        let now: i64 = 1_700_000_000;
        let last_check = now - (23 * 3600);
        // 23h since last check → not due yet, and the wait is the remainder.
        assert_eq!(
            next_sleep(true, false, Some(last_check), now),
            Some(std::time::Duration::from_secs(3600))
        );
        // Exactly 24h → due.
        assert_eq!(next_sleep(true, false, Some(now - 24 * 3600), now), None);
        // 25h → due.
        assert_eq!(next_sleep(true, false, Some(now - 25 * 3600), now), None);
    }

    /// Repeated start/stop within an hour does not fire multiple checks:
    /// model this by checking the cadence function with timestamps that
    /// represent successive launches all within the same window.
    #[test]
    fn scheduler_does_not_refire_within_one_hour() {
        let initial_check: i64 = 1_700_000_000;
        // Five "launches" all within 60 minutes of the initial check.
        for offset in &[60, 600, 1800, 3000, 3500] {
            let now = initial_check + offset;
            let next = next_sleep(true, false, Some(initial_check), now);
            assert!(
                next.is_some(),
                "scheduler must NOT fire at +{offset}s after last check"
            );
        }
    }

    /// Never-checked-before → fires on first wake.
    #[test]
    fn scheduler_fires_when_never_checked() {
        let now: i64 = 1_700_000_000;
        assert_eq!(next_sleep(true, false, None, now), None);
    }

    /// Phase 15 §Tests #7 — flipping paranoid_mode on suspends the
    /// scheduler regardless of how stale `last_checked_at` is, and leaves it
    /// idle-polling so turning Offline Mode off is noticed.
    #[test]
    fn scheduler_suspends_on_paranoid_mode() {
        let now: i64 = 1_700_000_000;
        assert_eq!(
            next_sleep(true, true, Some(now - 999_999), now),
            Some(AUTO_CHECK_IDLE_POLL)
        );
        assert_eq!(next_sleep(true, true, None, now), Some(AUTO_CHECK_IDLE_POLL));
    }

    /// Off by default: auto_check_enabled=false → never fires, but keeps
    /// polling so enabling it mid-session takes effect.
    #[test]
    fn scheduler_does_nothing_when_disabled() {
        let now: i64 = 1_700_000_000;
        assert_eq!(next_sleep(false, false, None, now), Some(AUTO_CHECK_IDLE_POLL));
        assert_eq!(
            next_sleep(false, false, Some(now - 100_000), now),
            Some(AUTO_CHECK_IDLE_POLL)
        );
    }

    /// Pin the backoff sequence at 1h, 6h, 24h. Drift here would mean
    /// silently changing the user-facing retry behaviour.
    #[test]
    fn auto_check_backoff_sequence_matches_plan_spec() {
        assert_eq!(AUTO_CHECK_BACKOFF.len(), 3);
        assert_eq!(AUTO_CHECK_BACKOFF[0].as_secs(), 60 * 60);
        assert_eq!(AUTO_CHECK_BACKOFF[1].as_secs(), 6 * 60 * 60);
        assert_eq!(AUTO_CHECK_BACKOFF[2].as_secs(), 24 * 60 * 60);
    }

    /// Pin the 24h floor.
    #[test]
    fn auto_check_interval_is_24_hours() {
        assert_eq!(AUTO_CHECK_INTERVAL.as_secs(), 24 * 60 * 60);
    }

    /// F-01 regression: the first wake must happen inside a normal session, not
    /// a full interval after launch. `last_checked_at` is memory-only, so a
    /// startup delay of `AUTO_CHECK_INTERVAL` meant the process was usually gone
    /// before the first check — and the scheduler is the only trigger for an
    /// unattended install.
    #[test]
    fn auto_check_startup_delay_fires_inside_a_normal_session() {
        assert_eq!(AUTO_CHECK_STARTUP_DELAY.as_secs(), 60);
        assert!(
            AUTO_CHECK_STARTUP_DELAY < AUTO_CHECK_INTERVAL,
            "the first check must not be deferred by a full interval"
        );
    }

    /// The startup sleep is the base delay plus bounded jitter: spread enough to
    /// avoid a thundering herd, small enough that the check still lands early in
    /// the session.
    #[test]
    fn startup_delay_adds_bounded_jitter() {
        let lo = AUTO_CHECK_STARTUP_DELAY;
        let hi = AUTO_CHECK_STARTUP_DELAY
            + std::time::Duration::from_secs(AUTO_CHECK_STARTUP_JITTER_SECS);
        for now in [0i64, 1, 29, 30, 1_700_000_000, -1_700_000_000] {
            let d = startup_delay(now);
            assert!(d >= lo && d < hi, "startup_delay({now}) = {d:?} out of range");
            // Deterministic for a given clock reading (no RNG to flake on).
            assert_eq!(d, startup_delay(now));
        }
        // And it actually varies across clock readings.
        assert_ne!(startup_delay(0), startup_delay(1));
    }

    /// F-01 regression: an inactive scheduler re-polls instead of sleeping a
    /// day, so flipping the toggle on mid-session takes effect without a
    /// relaunch (the toggle is otherwise a dead control).
    #[test]
    fn next_sleep_polls_soon_when_inactive() {
        let now: i64 = 1_700_000_000;
        assert_eq!(next_sleep(false, false, None, now), Some(AUTO_CHECK_IDLE_POLL));
        // Offline Mode wins over a stale opt-in and an ancient last check.
        assert_eq!(
            next_sleep(true, true, Some(now - 999_999), now),
            Some(AUTO_CHECK_IDLE_POLL)
        );
        // Opted in but never checked → run now, not idle-poll.
        assert_eq!(next_sleep(true, false, None, now), None);
    }

    /// Throttled wakes wait exactly the remaining floor. Returning a fresh
    /// interval would silently stretch the cadence to 48h when the floor expires
    /// mid-sleep.
    #[test]
    fn next_sleep_waits_the_remaining_floor_when_throttled() {
        let now: i64 = 1_700_000_000;
        assert_eq!(
            next_sleep(true, false, Some(now - 60), now),
            Some(AUTO_CHECK_INTERVAL - std::time::Duration::from_secs(60))
        );
        assert_eq!(
            next_sleep(true, false, Some(now - 23 * 3600), now),
            Some(std::time::Duration::from_secs(3600))
        );
        // At or past the floor → due.
        assert_eq!(
            next_sleep(true, false, Some(now - AUTO_CHECK_INTERVAL.as_secs() as i64), now),
            None
        );
        // A clock that jumped backwards must not panic or produce a negative
        // wait: treat it as "floor not reached" and wait the full interval.
        assert_eq!(
            next_sleep(true, false, Some(now + 500), now),
            Some(AUTO_CHECK_INTERVAL)
        );
    }

    /// The retry ladder is 1h → 6h → 24h and saturates at 24h, so a permanently
    /// failing check settles into one attempt per day instead of escalating
    /// forever.
    #[test]
    fn sleep_after_failure_walks_the_backoff_then_saturates() {
        assert_eq!(sleep_after_failure(0), std::time::Duration::from_secs(3600));
        assert_eq!(sleep_after_failure(1), std::time::Duration::from_secs(6 * 3600));
        assert_eq!(sleep_after_failure(2), std::time::Duration::from_secs(24 * 3600));
        assert_eq!(sleep_after_failure(9), AUTO_CHECK_INTERVAL);
        assert_eq!(sleep_after_success(), AUTO_CHECK_INTERVAL);
    }

    /// Backoff index advance saturates at the last step instead of growing
    /// unbounded (an out-of-range index would fall back to 24h anyway, but the
    /// index is also what the loop carries between wakes).
    #[test]
    fn next_backoff_index_saturates() {
        assert_eq!(next_backoff_index(0), 1);
        assert_eq!(next_backoff_index(1), 2);
        assert_eq!(next_backoff_index(2), 2);
        assert_eq!(next_backoff_index(99), 2);
    }

    // ---------- wire shape ----------

    /// UpdateCheckOutcome serializes with the `kind` tag in camelCase
    /// and `Available` carries the fields the frontend expects —
    /// including the two release-channel additions.
    #[test]
    fn update_check_outcome_wire_shape() {
        let v = serde_json::to_value(UpdateCheckOutcome::UpToDate).unwrap();
        assert_eq!(v["kind"], "upToDate");

        let v = serde_json::to_value(UpdateCheckOutcome::Available {
            version: "0.3.1".into(),
            current_version: "0.3.0".into(),
            notes: Some("changelog".into()),
            pub_date: Some("2026-05-24T00:00:00Z".into()),
            release_url: "https://github.com/deuriib/agency-agents-app/releases/tag/v0.3.1"
                .into(),
            installable: false,
            skipped: false,
        })
        .unwrap();
        assert_eq!(v["kind"], "available");
        assert_eq!(v["version"], "0.3.1");
        assert_eq!(v["currentVersion"], "0.3.0");
        assert_eq!(v["notes"], "changelog");
        assert_eq!(v["pubDate"], "2026-05-24T00:00:00Z");
        assert_eq!(
            v["releaseUrl"],
            "https://github.com/deuriib/agency-agents-app/releases/tag/v0.3.1"
        );
        assert_eq!(v["installable"], false);
        assert_eq!(v["skipped"], false);
    }

    /// `current_app_version` returns a parseable semver-shaped string.
    #[test]
    fn current_app_version_is_semver_shaped() {
        let v = current_app_version();
        assert!(semver::parse(v).is_some(), "{v} did not parse as semver");
    }

    // ---------- Auto-install (Phase 3) ----------

    /// Settings with auto-install explicitly on and Offline Mode off — the
    /// only state in which an unattended install is legal.
    fn auto_install_on() -> Settings {
        Settings {
            update_auto_check: true,
            update_auto_install: true,
            ..Settings::default()
        }
    }

    /// Seed the cache the way a preceding `run_check` would, so the
    /// `run_install` gates can run without a second mock round-trip.
    async fn seed_available(state: &AppState, version: &str, installable: bool) {
        let mut guard = state.updater_state.write().await;
        guard.cached_available = Some(CachedUpdate {
            version: version.to_string(),
            current_version: current_app_version().to_string(),
            notes: None,
            pub_date: None,
            release_url: format!(
                "https://github.com/deuriib/agency-agents-app/releases/tag/v{version}"
            ),
            installable,
        });
    }

    #[test]
    fn cancel_token_round_trips_request_and_reset() {
        let token = CancelToken::new();
        assert!(!token.is_requested(), "a fresh token starts un-cancelled");
        token.request();
        assert!(token.is_requested());
        token.reset();
        assert!(!token.is_requested(), "reset must arm the next attempt");
    }

    #[test]
    fn cancel_token_clones_share_the_same_flag() {
        let token = CancelToken::new();
        let clone = token.clone();
        clone.request();
        assert!(
            token.is_requested(),
            "the scheduler and the backend must observe the same flag"
        );
    }

    /// Default state: no opt-in → the blocker reports `Disabled` and the
    /// backend is never touched.
    #[tokio::test]
    async fn auto_install_blocker_reports_disabled_by_default() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        assert_eq!(
            auto_install_blocker(&state, "9.9.9").await,
            Some(AutoInstallSkip::Disabled)
        );
    }

    /// First launch (no settings file) must be treated as "not opted in".
    #[tokio::test]
    async fn auto_install_blocker_reports_disabled_on_first_launch() {
        let state = build_state_with(SettingsLoadState::FirstLaunch).await;
        assert_eq!(
            auto_install_blocker(&state, "9.9.9").await,
            Some(AutoInstallSkip::Disabled)
        );
    }

    /// Corrupt settings fail closed exactly like `require_network`.
    #[tokio::test]
    async fn auto_install_blocker_reports_offline_on_corrupt_settings() {
        let state = build_state_with(SettingsLoadState::Corrupt {
            message: "synthetic corruption".into(),
        })
        .await;
        assert_eq!(
            auto_install_blocker(&state, "9.9.9").await,
            Some(AutoInstallSkip::Offline)
        );
    }

    /// Opted in, but Offline Mode is on → the hard gate wins.
    #[tokio::test]
    async fn auto_install_blocker_reports_offline_when_paranoid_mode_is_on() {
        let s = Settings {
            paranoid_mode: true,
            ..auto_install_on()
        };
        let state = build_state_with(SettingsLoadState::Loaded(s)).await;
        assert_eq!(
            auto_install_blocker(&state, "9.9.9").await,
            Some(AutoInstallSkip::Offline)
        );
    }

    /// A skipped version is never auto-installed — the user dismissed it.
    #[tokio::test]
    async fn auto_install_blocker_reports_skipped_version() {
        let s = Settings {
            skipped_update_versions: vec!["9.9.9".into()],
            ..auto_install_on()
        };
        let state = build_state_with(SettingsLoadState::Loaded(s)).await;
        assert_eq!(
            auto_install_blocker(&state, "9.9.9").await,
            Some(AutoInstallSkip::SkippedVersion)
        );
        // A different version is not blocked by that entry.
        assert_eq!(auto_install_blocker(&state, "9.9.10").await, None);
    }

    /// Opted in, online, not skipped, nothing in flight → clear to proceed.
    #[tokio::test]
    async fn auto_install_blocker_clears_when_all_gates_pass() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        assert_eq!(auto_install_blocker(&state, "9.9.9").await, None);
    }

    /// Single-flight: a second attempt while one is running is refused rather
    /// than queued (ENG-11 — no unbounded concurrency).
    #[tokio::test]
    async fn auto_install_blocker_reports_already_running() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        {
            let mut guard = state.updater_state.write().await;
            guard.auto_install = AutoInstallStatus::Running {
                version: "9.9.9".into(),
            };
        }
        assert_eq!(
            auto_install_blocker(&state, "9.9.9").await,
            Some(AutoInstallSkip::AlreadyRunning)
        );
    }

    // ---------- install slot (F-02) ----------

    /// The slot is claimed at most once. `claim_install_slot` is what makes the
    /// single-flight guarantee real: the gate read and the `Running` write happen
    /// in one lock section, so two callers cannot both pass.
    #[tokio::test]
    async fn claim_install_slot_is_single_flight() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;

        assert!(claim_install_slot(&state, "9.9.9").await, "first claim wins");
        assert!(
            !claim_install_slot(&state, "9.9.9").await,
            "a second claim must be refused while one is running"
        );
        assert!(
            !claim_install_slot(&state, "9.9.10").await,
            "the slot is global, not per-version"
        );
        assert_eq!(
            state.updater_state.read().await.auto_install,
            AutoInstallStatus::Running {
                version: "9.9.9".into()
            },
            "the refused claim must not overwrite the running version"
        );
    }

    /// Releasing frees the slot for the next attempt.
    #[tokio::test]
    async fn release_install_slot_frees_the_slot() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;

        assert!(claim_install_slot(&state, "9.9.9").await);
        release_install_slot(&state).await;
        assert_eq!(state.updater_state.read().await.auto_install, AutoInstallStatus::Idle);
        assert!(
            claim_install_slot(&state, "9.9.10").await,
            "a released slot must be claimable again"
        );
    }

    /// Releasing must not erase a terminal status the caller already recorded —
    /// that status is what drives the relaunch prompt and the error text.
    #[tokio::test]
    async fn release_install_slot_preserves_terminal_status() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        set_auto_install_status(
            &state,
            AutoInstallStatus::Installed {
                version: "9.9.9".into(),
            },
        )
        .await;

        release_install_slot(&state).await;

        assert_eq!(
            state.updater_state.read().await.auto_install,
            AutoInstallStatus::Installed {
                version: "9.9.9".into()
            }
        );
    }

    /// F-02 regression at the auto-install entry point: when a manual install
    /// already holds the slot, the background attempt is refused *before* it
    /// touches the backend, so two bundle swaps cannot race.
    #[tokio::test]
    async fn auto_install_skips_when_another_install_holds_the_slot() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", true).await;
        // Simulates `update_install` having claimed the slot a moment ago.
        assert!(claim_install_slot(&state, "9.9.9").await);

        let backend = MockBackend::returning(Ok(None));
        let r = run_auto_install(&state, &backend, "9.9.9")
            .await
            .expect("attempt");

        assert_eq!(r, AutoInstallResult::Skipped(AutoInstallSkip::AlreadyRunning));
        assert_eq!(
            *backend.install_calls.lock().unwrap(),
            0,
            "a held slot must keep the background path off the bundle swap"
        );
        // The holder's claim survives: the refused attempt must not release it.
        assert!(matches!(
            state.updater_state.read().await.auto_install,
            AutoInstallStatus::Running { .. }
        ));
    }

    /// The manual path takes the same slot: happy path installs, then frees it.
    #[tokio::test]
    async fn manual_install_claims_and_releases_the_slot() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", true).await;
        let backend = MockBackend::returning(Ok(None));

        run_manual_install(&state, &backend, "9.9.9")
            .await
            .expect("manual install");

        assert_eq!(*backend.install_calls.lock().unwrap(), 1);
        assert_eq!(
            state.updater_state.read().await.auto_install,
            AutoInstallStatus::Idle,
            "a finished manual install must free the slot"
        );
        assert!(
            state.updater_state.read().await.cached_available.is_none(),
            "a completed install clears the cached available entry"
        );
    }

    /// F-02 regression, manual side: a background install already holds the
    /// slot → the manual command is refused with a typed error and never
    /// reaches the bundle swap.
    #[tokio::test]
    async fn manual_install_is_refused_while_the_slot_is_held() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", true).await;
        // Simulates the scheduler having claimed the slot mid-download.
        assert!(claim_install_slot(&state, "9.9.9").await);
        let backend = MockBackend::returning(Ok(None));

        let r = run_manual_install(&state, &backend, "9.9.9").await;

        assert!(
            matches!(r, Err(AppError::InstallInProgress)),
            "expected InstallInProgress, got {r:?}"
        );
        assert_eq!(
            *backend.install_calls.lock().unwrap(),
            0,
            "a refused manual install must not touch the backend"
        );
        assert!(
            matches!(
                state.updater_state.read().await.auto_install,
                AutoInstallStatus::Running { .. }
            ),
            "the holder's claim must survive a refused manual attempt"
        );
    }

    /// A failed manual install must still free the slot, or the user could never
    /// retry after a transient network error.
    #[tokio::test]
    async fn manual_install_releases_the_slot_on_failure() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", true).await;
        let backend = MockBackend::install_returning(
            Ok(None),
            Err(AppError::Network {
                url: "update download".into(),
                message: "connection reset".into(),
            }),
        );

        let r = run_manual_install(&state, &backend, "9.9.9").await;

        assert!(matches!(r, Err(AppError::Network { .. })));
        assert_eq!(
            state.updater_state.read().await.auto_install,
            AutoInstallStatus::Idle,
            "a failure must not leave the single-flight guard armed"
        );
    }

    /// A manual install that fails a gate (stale version) also frees the slot.
    #[tokio::test]
    async fn manual_install_releases_the_slot_on_gate_refusal() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", true).await;
        let backend = MockBackend::returning(Ok(None));

        let r = run_manual_install(&state, &backend, "9.9.10").await;

        assert!(matches!(r, Err(AppError::InvalidArgument { .. })));
        assert_eq!(*backend.install_calls.lock().unwrap(), 0);
        assert_eq!(
            state.updater_state.read().await.auto_install,
            AutoInstallStatus::Idle
        );
    }

    /// Disabled opt-in: the install is skipped and the backend never runs.
    #[tokio::test]
    async fn auto_install_skips_disabled_without_calling_backend() {
        let state = build_state_with(SettingsLoadState::Loaded(Settings::default())).await;
        seed_available(&state, "9.9.9", true).await;
        let backend = MockBackend::returning(Ok(None));

        let r = run_auto_install(&state, &backend, "9.9.9")
            .await
            .expect("attempt");
        assert_eq!(r, AutoInstallResult::Skipped(AutoInstallSkip::Disabled));
        assert_eq!(
            *backend.install_calls.lock().unwrap(),
            0,
            "a disabled opt-in must not reach the install backend"
        );
    }

    /// Offline Mode blocks the attempt before the backend is touched.
    #[tokio::test]
    async fn auto_install_skips_offline_without_calling_backend() {
        let s = Settings {
            paranoid_mode: true,
            ..auto_install_on()
        };
        let state = build_state_with(SettingsLoadState::Loaded(s)).await;
        seed_available(&state, "9.9.9", true).await;
        let backend = MockBackend::returning(Ok(None));

        let r = run_auto_install(&state, &backend, "9.9.9")
            .await
            .expect("attempt");
        assert_eq!(r, AutoInstallResult::Skipped(AutoInstallSkip::Offline));
        assert_eq!(*backend.install_calls.lock().unwrap(), 0);
    }

    /// Happy path: opted in + online + available → installed, and the cached
    /// entry is cleared so the UI stops offering the same install.
    #[tokio::test]
    async fn auto_install_installs_when_all_gates_pass() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", true).await;
        let backend = MockBackend::returning(Ok(None));

        let r = run_auto_install(&state, &backend, "9.9.9")
            .await
            .expect("attempt");
        assert_eq!(r, AutoInstallResult::Installed);
        assert_eq!(*backend.install_calls.lock().unwrap(), 1);

        let guard = state.updater_state.read().await;
        assert_eq!(
            guard.auto_install,
            AutoInstallStatus::Installed {
                version: "9.9.9".into()
            }
        );
        assert!(
            guard.cached_available.is_none(),
            "a completed install must clear the cached available entry"
        );
    }

    /// Cancellation: the backend reports `InstallCanceled` → the attempt is
    /// `Canceled`, the cache survives (so a manual install stays possible),
    /// and no failure is recorded.
    #[tokio::test]
    async fn auto_install_maps_cancel_and_preserves_cache() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", true).await;
        let backend =
            MockBackend::install_returning(Ok(None), Err(AppError::InstallCanceled));

        let r = run_auto_install(&state, &backend, "9.9.9")
            .await
            .expect("attempt");
        assert_eq!(r, AutoInstallResult::Canceled);

        let guard = state.updater_state.read().await;
        assert_eq!(
            guard.auto_install,
            AutoInstallStatus::Canceled {
                version: "9.9.9".into()
            }
        );
        assert!(
            guard.cached_available.is_some(),
            "a canceled install must leave the manual path intact"
        );
    }

    /// A gate refusal inside `run_install` (here: no in-app manifest) is a
    /// skip, not a failure — the user gets no error toast for it.
    #[tokio::test]
    async fn auto_install_maps_gate_refusal_to_skip() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", false).await;
        let backend = MockBackend::returning(Ok(None));

        let r = run_auto_install(&state, &backend, "9.9.9")
            .await
            .expect("attempt");
        assert_eq!(r, AutoInstallResult::Skipped(AutoInstallSkip::NotInstallable));
        assert_eq!(*backend.install_calls.lock().unwrap(), 0);

        let guard = state.updater_state.read().await;
        assert_eq!(
            guard.auto_install,
            AutoInstallStatus::Idle,
            "a skip must not leave the single-flight guard armed"
        );
    }

    /// A real failure (signature verification) is surfaced AND recorded, so
    /// the Settings card can show the typed message.
    #[tokio::test]
    async fn auto_install_records_failure_and_returns_error() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        seed_available(&state, "9.9.9", true).await;
        let backend = MockBackend::install_returning(
            Ok(None),
            Err(AppError::SignatureVerificationFailed {
                message: "minisign rejected".into(),
            }),
        );

        let r = run_auto_install(&state, &backend, "9.9.9").await;
        assert!(matches!(r, Err(AppError::SignatureVerificationFailed { .. })));

        let guard = state.updater_state.read().await;
        match &guard.auto_install {
            AutoInstallStatus::Failed { version, message } => {
                assert_eq!(version, "9.9.9");
                assert!(message.contains("signature"), "got: {message}");
            }
            other => panic!("expected Failed status, got {other:?}"),
        }
    }

    /// Revoking consent — or engaging Offline Mode — requests a cancel. Every
    /// other transition is inert (a spurious request would abort an unrelated
    /// future install).
    #[tokio::test]
    async fn auto_install_consent_change_requests_cancel_only_on_revocation() {
        let state = build_state_with(SettingsLoadState::Loaded(auto_install_on())).await;
        let on_online = AutoInstallConsent {
            enabled: true,
            offline: false,
        };
        let off_online = AutoInstallConsent {
            enabled: false,
            offline: false,
        };
        let on_offline = AutoInstallConsent {
            enabled: true,
            offline: true,
        };

        assert!(
            handle_auto_install_consent_change(&state, on_online, off_online).await,
            "auto-install on -> off must request cancellation"
        );
        assert!(state.updater_state.read().await.auto_install_cancel.is_requested());

        // Re-arm and confirm Offline Mode engaging is a withdrawal too.
        state.updater_state.read().await.auto_install_cancel.reset();
        assert!(
            handle_auto_install_consent_change(&state, on_online, on_offline).await,
            "Offline Mode off -> on must request cancellation"
        );

        // Re-arm and confirm the inert transitions stay inert.
        state.updater_state.read().await.auto_install_cancel.reset();
        assert!(!handle_auto_install_consent_change(&state, off_online, on_online).await);
        assert!(!handle_auto_install_consent_change(&state, off_online, off_online).await);
        assert!(
            !handle_auto_install_consent_change(&state, on_offline, on_offline).await,
            "staying offline is not a transition"
        );
        assert!(
            !state.updater_state.read().await.auto_install_cancel.is_requested(),
            "enabling must not request a cancellation"
        );
    }

    /// Wire shape: the frontend switches on `phase` and reads `version` /
    /// `message`. A drift here silently breaks the pill + Settings card.
    #[test]
    fn auto_install_event_wire_shape() {
        let v = serde_json::to_value(AutoInstallEvent {
            phase: AutoInstallPhase::Verified,
            version: "0.6.0".into(),
            message: None,
        })
        .unwrap();
        assert_eq!(v["phase"], "verified");
        assert_eq!(v["version"], "0.6.0");
        assert!(v.get("message").is_none(), "message is omitted when absent");

        let v = serde_json::to_value(AutoInstallEvent {
            phase: AutoInstallPhase::Failed,
            version: "0.6.0".into(),
            message: Some("signature rejected".into()),
        })
        .unwrap();
        assert_eq!(v["phase"], "failed");
        assert_eq!(v["message"], "signature rejected");
    }

    /// Every phase serializes to the exact lowercase token the renderer
    /// matches on — a rename here is a silent UI regression.
    #[test]
    fn auto_install_phases_serialize_as_expected_tokens() {
        let tokens: Vec<String> = [
            AutoInstallPhase::Started,
            AutoInstallPhase::Verified,
            AutoInstallPhase::Installed,
            AutoInstallPhase::Failed,
            AutoInstallPhase::Canceled,
        ]
        .iter()
        .map(|p| serde_json::to_value(p).unwrap().as_str().unwrap().to_string())
        .collect();
        assert_eq!(
            tokens,
            vec!["started", "verified", "installed", "failed", "canceled"]
        );
    }

    /// The status enum's `phase` tag must match the event tokens so the UI can
    /// render from either source without a translation table.
    #[test]
    fn auto_install_status_serializes_with_phase_tag() {
        let v = serde_json::to_value(AutoInstallStatus::Running {
            version: "0.6.0".into(),
        })
        .unwrap();
        assert_eq!(v["phase"], "running");
        assert_eq!(v["version"], "0.6.0");

        let v = serde_json::to_value(AutoInstallStatus::Idle).unwrap();
        assert_eq!(v["phase"], "idle");
    }
}
