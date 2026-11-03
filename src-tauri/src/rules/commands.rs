//! Tauri command surface for the rules corpus.
//!
//! W4 (TASK 12) registers two commands: [`rules_status`], the only source of the
//! "not synced" vs "no rules shipped" distinction (spec §8), and
//! [`rules_refresh`], the action the `notSynced` empty state's CTA needs
//! (ruling C8). The rest of the rules surface (`rules_list`, `rules_get`,
//! `rules_preview`, `rules_install`, `rules_uninstall`) lands in W6 and reads
//! through the same corpus module.
//!
//! Both commands live here rather than in `src-tauri/src/commands/`, following
//! the `corpus/` precedent (ruling A1).

use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::corpus::{app_data_dir, resource_dir};
use crate::error::AppError;
use crate::rules::corpus;
use crate::state::AppState;
use crate::types::RuleStatus;

/// The bundled rules baseline: `<resources>/resources/rules-baseline`, whose
/// `rules/` subtree is copied into the working copy on first launch (TASK 12).
fn rules_baseline_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    Ok(resource_dir(app)?.join("resources").join("rules-baseline"))
}

/// `rules_status()` — seed on first use, then report
/// `{ configured, count, version, emptyReason }`.
///
/// Local-only: no network. A missing corpus is a state (`emptyReason =
/// "notSynced"`), not an error, so the Rules section can always render.
#[tauri::command]
pub async fn rules_status(app: AppHandle) -> Result<RuleStatus, AppError> {
    let app_data = app_data_dir(&app)?;
    let baseline = rules_baseline_dir(&app)?;
    corpus::status(&app_data, &baseline).await
}

/// `rules_refresh()` — fetch the `agency-rules` tarball, atomically swap the
/// working copy, rebuild `rules-index.json`, and return the fresh status.
///
/// Offline Mode is a hard gate (`require_network` first, mirroring
/// `corpus_refresh`), and a second click while one refresh runs fast-fails
/// instead of queuing a duplicate download.
#[tauri::command]
pub async fn rules_refresh(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RuleStatus, AppError> {
    state.require_network("rules_refresh").await?;

    let _flight = match state.rules_refresh_in_flight.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            return Err(AppError::InvalidArgument {
                message: "rules refresh already in progress".into(),
            })
        }
    };

    let app_data = app_data_dir(&app)?;
    corpus::refresh(&app_data).await
}
