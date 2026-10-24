import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UpdateStatus } from "$lib/tauri";

/**
 * Update store: a mirror of the backend's updater state machine.
 *
 * The store never touches the network itself — every call goes through the
 * typed bridge, and the payload is signature-verified in Rust before it is
 * installed (G-UPD-01). What matters here:
 * - one value, emitted whole: a status event replaces the previous one, the
 *   UI never stitches phases together;
 * - a rejected command surfaces its reason rather than leaving the button
 *   silently dead;
 * - the busy flag serialises commands — a second click while one is in
 *   flight is ignored, not queued.
 */
vi.mock("$lib/tauri", async () => {
  const { tauriMock } = await import("./tauri-mock");
  return tauriMock;
});

import { listeners, resetTauriMock, tauriMock } from "./tauri-mock";
import { updateStore } from "./update.svelte";

function status(phase: UpdateStatus["phase"], over: Partial<UpdateStatus> = {}): UpdateStatus {
  return {
    current_version: "0.4.0",
    phase,
    available_version: null,
    notes: null,
    checked_at_unix: null,
    progress_percent: null,
    error: null,
    ...over,
  };
}

beforeEach(() => {
  resetTauriMock();
  updateStore.status = null;
  updateStore.busy = false;
  updateStore.error = null;
});

describe("refresh", () => {
  it("reads the current state without a network call", async () => {
    tauriMock.getUpdateStatus.mockResolvedValueOnce(status("up_to_date"));
    await updateStore.refresh();
    expect(updateStore.status?.phase).toBe("up_to_date");
    expect(updateStore.error).toBeNull();
  });

  it("surfaces a failure as an error string", async () => {
    tauriMock.getUpdateStatus.mockRejectedValueOnce(new Error("offline"));
    await updateStore.refresh();
    expect(updateStore.status).toBeNull();
    expect(updateStore.error).toBe("Error: offline");
  });
});

describe("check / install", () => {
  it("check replaces the status and settles busy", async () => {
    tauriMock.checkForUpdate.mockResolvedValueOnce(
      status("available", { available_version: "0.5.0" }),
    );
    await updateStore.check();
    expect(updateStore.status?.phase).toBe("available");
    expect(updateStore.status?.available_version).toBe("0.5.0");
    expect(updateStore.busy).toBe(false);
    expect(tauriMock.checkForUpdate).toHaveBeenCalledTimes(1);
  });

  it("a second click while busy is ignored, not queued", async () => {
    let release!: (s: UpdateStatus) => void;
    tauriMock.checkForUpdate.mockImplementationOnce(
      () => new Promise<UpdateStatus>((r) => (release = r)),
    );
    const first = updateStore.check();
    const second = updateStore.check(); // must no-op
    release(status("up_to_date"));
    await Promise.all([first, second]);
    expect(tauriMock.checkForUpdate).toHaveBeenCalledTimes(1);
    expect(updateStore.busy).toBe(false);
  });

  it("a rejected install shows its reason instead of a silent dead button", async () => {
    tauriMock.installUpdate.mockRejectedValueOnce(new Error("scan running"));
    await updateStore.install();
    expect(updateStore.error).toBe("Error: scan running");
    expect(updateStore.busy).toBe(false);
  });

  it("a rejected restart records the reason (the process usually never returns)", async () => {
    tauriMock.restartApp.mockRejectedValueOnce(new Error("relaunch refused"));
    await updateStore.restart();
    expect(updateStore.error).toBe("Error: relaunch refused");
  });
});

describe("listen", () => {
  it("mirrors each whole status the backend emits", () => {
    updateStore.listen();
    listeners.updateStatus?.(status("checking"));
    expect(updateStore.status?.phase).toBe("checking");

    listeners.updateStatus?.(status("downloading", { progress_percent: 40 }));
    expect(updateStore.status?.progress_percent).toBe(40);

    listeners.updateStatus?.(status("ready_to_restart"));
    expect(updateStore.status?.phase).toBe("ready_to_restart");
  });
});
