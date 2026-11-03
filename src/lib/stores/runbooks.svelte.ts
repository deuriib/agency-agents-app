/**
 * Runbooks store — the NEXUS scenario runbooks from the catalog's
 * `strategy/runbooks.json` (catalog PR #664). Each runbook names its roster BY
 * SLUG; the UI resolves those against the loaded corpus to deploy the set.
 *
 * An empty manifest is the normal "catalog ships no strategy/" state (the
 * offline bundled baseline), so the UI nudges toward syncing. A manifest that
 * exists but cannot be read or parsed surfaces `error` instead: silently
 * degrading to "not synced" hid a broken catalog behind a misleading CTA (#58).
 *
 * Singleton: import `runbooks` everywhere.
 */
import { invoke } from "@tauri-apps/api/core";

import { errorText, type Runbook } from "$lib/types";

class RunbooksStore {
  /** The scenario runbooks, in manifest order. Empty until loaded / when unsynced. */
  list: Runbook[] = $state([]);
  /** True once the first load resolves (so "empty" ≠ "not fetched yet"). */
  loaded: boolean = $state(false);
  /** True while a load is in flight. */
  loading: boolean = $state(false);
  /** Manifest read/parse failure, ready to show. `null` when the load succeeded. */
  error: string | null = $state(null);

  /** Load the manifest from the active catalog. Safe to call on mount. */
  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    try {
      this.list = await invoke<Runbook[]>("runbooks_list");
      this.error = null;
    } catch (e) {
      this.list = [];
      this.error = errorText(e);
    } finally {
      this.loaded = true;
      this.loading = false;
    }
  }
}

export const runbooks = new RunbooksStore();
