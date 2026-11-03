import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/svelte";
import { afterEach } from "vitest";

/** Every test renders into a fresh document; Svelte Testing Library owns the teardown. */
afterEach(() => {
  cleanup();
});
