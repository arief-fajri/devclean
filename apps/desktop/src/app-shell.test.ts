// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * G-UI-03 — the confirmation is an interruption, never a page.
 *
 * `architecture.test.ts` proves no *page* can call the cleanup API. That
 * cannot see the other half of the rule: while the confirmation flow is
 * open, the SHELL must keep its hands off the keyboard — Escape belongs to
 * the dialog, and a shortcut must never navigate away from an unreviewed
 * destruction. This test renders the real `App.svelte` and fires the real
 * keystrokes, so the guard has behavioural evidence, not just strings in
 * the source.
 */
vi.mock("$lib/tauri", async () => {
  const { tauriMock } = await import("$lib/stores/tauri-mock");
  return tauriMock;
});

import { cleanupStore } from "$lib/stores/cleanup.svelte";
import { navStore } from "$lib/stores/nav.svelte";
import { scanStore } from "$lib/stores/scan.svelte";
import { settingsStore } from "$lib/stores/settings.svelte";
import App from "./App.svelte";

beforeEach(async () => {
  const { resetTauriMock } = await import("$lib/stores/tauri-mock");
  resetTauriMock();
  navStore.route = "overview";
  navStore.settingsTab = "providers";
  navStore.pendingSearchFocus = false;
  cleanupStore.confirmOpen = false;
  cleanupStore.previewing = false;
  cleanupStore.preview = null;
  cleanupStore.previewError = null;
  cleanupStore.previewToken = null;
  cleanupStore.cleaning = false;
  cleanupStore.results = null;
  scanStore.summary = null;
  scanStore.selected = [];
  scanStore.scanning = false;
  scanStore.error = null;
  settingsStore.settings = null;
  await tick();
});

async function shell() {
  const out = render(App);
  await tick();
  return out;
}

const press = (key: string) => fireEvent.keyDown(window, { key, metaKey: true });

describe("G-UI-03 — the shell keeps its hands off the keyboard while the flow is open", () => {
  it("renders the shell with the sidebar and the Overview page", async () => {
    await shell();
    expect(screen.getByRole("navigation", { name: "Pages" })).toBeInTheDocument();
    expect(navStore.route).toBe("overview");
  });

  it("⌘3 navigates when no confirmation is open", async () => {
    await shell();
    expect(navStore.route).toBe("overview");
    await press("3");
    await tick();
    expect(navStore.route).toBe("history");
  });

  it("⌘1 does NOT navigate while the confirmation is open", async () => {
    await shell();
    await press("3");
    await tick();
    expect(navStore.route).toBe("history");

    // The confirmation opens — from the user's click, before any measure.
    cleanupStore.confirmOpen = true;
    await tick();
    // The dialog is really mounted (Escape must have somewhere to go).
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    await press("1");
    await tick();
    expect(
      navStore.route,
      "a shortcut must never navigate away from an unreviewed destruction",
    ).toBe("history");

    await press("4");
    await tick();
    expect(navStore.route).toBe("history");
  });

  it("the keyboard comes back once the flow closes", async () => {
    await shell();
    cleanupStore.confirmOpen = true;
    await tick();

    await press("2");
    await tick();
    expect(navStore.route).toBe("overview");

    cleanupStore.closeFlow();
    await tick();
    await press("2");
    await tick();
    expect(navStore.route).toBe("resources");
  });

  it("clicking a sidebar page moves the shell", async () => {
    await shell();
    await screen.getByRole("button", { name: /History/ }).click();
    await waitFor(() => expect(navStore.route).toBe("history"));
    expect(screen.getByRole("button", { name: /History/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("a text field still owns its keystrokes (⌘2 must not jump pages)", async () => {
    await shell();
    const input = document.createElement("input");
    document.body.append(input);
    input.focus();
    await fireEvent.keyDown(input, { key: "2", metaKey: true });
    expect(navStore.route).toBe("overview");
    input.remove();
  });
});
