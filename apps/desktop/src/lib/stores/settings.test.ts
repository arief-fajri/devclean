import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "$lib/tauri";

/**
 * Settings store: load, save (with the rescan-need answer), detect, theme.
 *
 * The contract this pins:
 * - `save` returns whether the caller must rescan — roots, exclusions,
 *   provider set or retention changed — so a theme flip never triggers a
 *   full rescan and a root change always does;
 * - a root change also refreshes the candidate list, so the Appearance tab
 *   reflects what was just saved;
 * - `refreshDiskFree` failure leaves the previous reading in place rather
 *   than blanking the card (stale is better than nothing);
 * - `detect` guards against re-entry while a discovery is in flight.
 */
vi.mock("$lib/tauri", async () => {
  const { tauriMock } = await import("./tauri-mock");
  return tauriMock;
});

import { resetTauriMock, tauriMock } from "./tauri-mock";
import { settingsStore } from "./settings.svelte";

function settings(over: Partial<Settings> = {}): Settings {
  return {
    scan_roots: ["/Users/me/Projects"],
    excluded_paths: [],
    require_confirmation: true,
    show_preview: true,
    allow_privileged_operations: false,
    scan_automatically: false,
    theme: "system",
    disabled_providers: [],
    retention_days: null,
    docker_prune_scope: "off",
    sqlite_backup: false,
    ai_cleanup_enabled: false,
    check_updates_automatically: false,
    ...over,
  };
}

beforeEach(() => {
  resetTauriMock();
  settingsStore.settings = null;
  settingsStore.providers = [];
  settingsStore.saving = false;
  settingsStore.error = null;
  settingsStore.candidates = [];
  settingsStore.detecting = false;
  settingsStore.diskFree = null;
});

describe("load", () => {
  it("populates settings, providers and free space", async () => {
    tauriMock.getSettings.mockResolvedValueOnce(settings());
    tauriMock.getProviders.mockResolvedValueOnce([
      { id: "node_modules", name: "node_modules", enabled: true },
    ]);
    tauriMock.getDiskFree.mockResolvedValueOnce({
      available_bytes: 500,
      total_bytes: 1000,
      known: true,
      mount: null,
    });
    await settingsStore.load();
    expect(settingsStore.settings?.theme).toBe("system");
    expect(settingsStore.providers).toHaveLength(1);
    expect(settingsStore.diskFree?.available_bytes).toBe(500);
    expect(settingsStore.error).toBeNull();
  });

  it("surfaces a failure without throwing", async () => {
    tauriMock.getSettings.mockRejectedValueOnce(new Error("nope"));
    await settingsStore.load();
    expect(settingsStore.error).toBe("Error: nope");
  });
});

describe("save", () => {
  it("asks the caller to rescan when the roots changed", async () => {
    settingsStore.settings = settings({ scan_roots: ["/old"] });
    const next = settings({ scan_roots: ["/old", "/new"] });
    tauriMock.updateSettings.mockResolvedValueOnce(next);
    tauriMock.getProviders.mockResolvedValueOnce([]);
    tauriMock.detectScanRoots.mockResolvedValueOnce([]);

    const changed = await settingsStore.save(next);
    expect(changed).toBe(true);
    expect(tauriMock.detectScanRoots).toHaveBeenCalled();
    expect(settingsStore.settings?.scan_roots).toEqual(["/old", "/new"]);
  });

  it("does not ask for a rescan when only the theme changed", async () => {
    settingsStore.settings = settings();
    const next = settings({ theme: "dark" });
    tauriMock.updateSettings.mockResolvedValueOnce(next);
    tauriMock.getProviders.mockResolvedValueOnce([]);

    const changed = await settingsStore.save(next);
    expect(changed).toBe(false);
    // Candidates are not refreshed for a change that cannot affect them.
    expect(tauriMock.detectScanRoots).not.toHaveBeenCalled();
  });

  it("asks for a rescan when retention changed", async () => {
    settingsStore.settings = settings({ retention_days: null });
    const next = settings({ retention_days: 7 });
    tauriMock.updateSettings.mockResolvedValueOnce(next);
    tauriMock.getProviders.mockResolvedValueOnce([]);

    expect(await settingsStore.save(next)).toBe(true);
  });

  it("asks for a rescan when the disabled provider set changed", async () => {
    settingsStore.settings = settings({ disabled_providers: [] });
    const next = settings({ disabled_providers: ["docker"] });
    tauriMock.updateSettings.mockResolvedValueOnce(next);
    tauriMock.getProviders.mockResolvedValueOnce([]);

    expect(await settingsStore.save(next)).toBe(true);
  });

  it("sorts before comparing, so an order-only change is not a rescan", async () => {
    settingsStore.settings = settings({ scan_roots: ["/a", "/b"] });
    const next = settings({ scan_roots: ["/b", "/a"] });
    tauriMock.updateSettings.mockResolvedValueOnce(next);
    tauriMock.getProviders.mockResolvedValueOnce([]);

    expect(await settingsStore.save(next)).toBe(false);
  });

  it("returns false and records the error when the bridge rejects", async () => {
    settingsStore.settings = settings();
    tauriMock.updateSettings.mockRejectedValueOnce(new Error("disk full"));
    const changed = await settingsStore.save(settings({ theme: "dark" }));
    expect(changed).toBe(false);
    expect(settingsStore.error).toBe("Error: disk full");
    expect(settingsStore.saving).toBe(false);
  });
});

describe("detect", () => {
  it("replaces the candidate list", async () => {
    tauriMock.detectScanRoots.mockResolvedValueOnce([
      { path: "/Users/me/Projects", approximate_bytes: 10, cache_count: 2, already_configured: true },
    ]);
    await settingsStore.detect();
    expect(settingsStore.candidates).toHaveLength(1);
    expect(settingsStore.detecting).toBe(false);
  });

  it("guards against re-entry while a discovery is in flight", async () => {
    let release!: (c: Awaited<ReturnType<typeof tauriMock.detectScanRoots>>) => void;
    tauriMock.detectScanRoots.mockImplementationOnce(
      () => new Promise((r) => (release = r)),
    );
    const first = settingsStore.detect();
    const second = settingsStore.detect(); // must no-op
    release([]);
    await Promise.all([first, second]);
    expect(tauriMock.detectScanRoots).toHaveBeenCalledTimes(1);
  });

  it("surfaces a failure without throwing", async () => {
    tauriMock.detectScanRoots.mockRejectedValueOnce(new Error("denied"));
    await settingsStore.detect();
    expect(settingsStore.error).toBe("Error: denied");
    expect(settingsStore.detecting).toBe(false);
  });
});

describe("refreshDiskFree", () => {
  it("keeps the previous reading when the new one fails", async () => {
    settingsStore.diskFree = {
      available_bytes: 42,
      total_bytes: 100,
      known: true,
      mount: null,
    };
    tauriMock.getDiskFree.mockRejectedValueOnce(new Error("statvfs failed"));
    await settingsStore.refreshDiskFree();
    expect(settingsStore.diskFree?.available_bytes).toBe(42);
  });
});

describe("setTheme", () => {
  it("is a no-op before settings are loaded", async () => {
    await settingsStore.setTheme("dark");
    expect(tauriMock.updateSettings).not.toHaveBeenCalled();
  });

  it("persists the theme without touching anything else", async () => {
    settingsStore.settings = settings();
    tauriMock.updateSettings.mockResolvedValueOnce(settings({ theme: "dark" }));
    await settingsStore.setTheme("dark");
    expect(settingsStore.settings?.theme).toBe("dark");
  });
});
