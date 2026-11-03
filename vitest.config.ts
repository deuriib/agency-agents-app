import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { resolve } from "node:path";
import { defineConfig } from "vitest/config";

/**
 * Frontend unit + component tests (issue #62).
 *
 * Deliberately NOT merged with `vite.config.js`: that one loads SvelteKit, whose
 * plugin wants a synced `.svelte-kit` build. These tests need the plain Svelte
 * transform plus a `$lib` alias, and nothing else — a unit suite that depends on
 * the app build is a unit suite that fails for reasons unrelated to the change.
 */
export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  resolve: {
    alias: {
      $lib: resolve(import.meta.dirname, "src/lib"),
    },
    // Svelte 5 ships separate client/server entries; jsdom needs the client one.
    conditions: ["browser"],
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    setupFiles: ["./src/test/setup.ts"],
    restoreMocks: true,
    clearMocks: true,
  },
});
