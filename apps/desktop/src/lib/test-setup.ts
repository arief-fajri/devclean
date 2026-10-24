/**
 * Vitest setup: jest-dom matchers (`toBeDisabled`, `toBeVisible`, …) and
 * testing-library cleanup.
 *
 * Cleanup runs explicitly because the suite does not enable vitest globals —
 * without it every render stays mounted in `document.body` and the next test
 * finds two of everything.
 *
 * Loaded for every test file; the matchers only matter in the DOM
 * environment, and registering them in node costs nothing.
 */
import { cleanup } from "@testing-library/svelte";
import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";

afterEach(() => cleanup());
