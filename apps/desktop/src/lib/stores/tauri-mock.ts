/**
 * Shared `$lib/tauri` mock for the store tests.
 *
 * The stores are singletons wired to the real bridge at import time, so each
 * test file replaces the whole module and drives these fns directly. Every
 * command defaults to rejecting with a clear "not stubbed" message: a test
 * that forgets to stub a call fails loudly instead of silently receiving
 * `undefined`.
 *
 * The `on*` listeners record their callback here so a test can fire a Tauri
 * event by hand — the stores only ever see the callback, never the transport.
 */
import { vi } from "vitest";
import type {
  CachedScan,
  CleanupPreview,
  CleanupResult,
  CleanupRun,
  DiskFree,
  ProviderInfo,
  Resource,
  ScanRootCandidate,
  ScanSummary,
  Settings,
  UpdateStatus,
} from "$lib/tauri";

export const listeners: {
  scanStarted?: () => void;
  scanProviderStarted?: (provider: string) => void;
  scanProgress?: (p: { provider: string; done: number; total: number }) => void;
  scanCompleted?: (s: ScanSummary) => void;
  cleanupStarted?: (ids: string[]) => void;
  cleanupProgress?: (p: { done: number; total: number }) => void;
  cleanupCompleted?: (r: CleanupResult[]) => void;
  updateStatus?: (s: UpdateStatus) => void;
  trayAction?: (a: string) => void;
} = {};

const listen = <T>(slot: keyof typeof listeners) =>
  vi.fn(async (cb: (payload: T) => void): Promise<() => void> => {
    (listeners as Record<string, unknown>)[slot] = cb;
    return () => {};
  });

export const tauriMock = {
  // ---- scan
  scanAll: vi.fn((): Promise<ScanSummary> =>
    Promise.reject(new Error("scan_all not stubbed")),
  ),
  scanProvider: vi.fn((_provider: string): Promise<ScanSummary> =>
    Promise.reject(new Error("scan_provider not stubbed")),
  ),
  getResourceDetails: vi.fn((_id: string): Promise<Resource | null> =>
    Promise.reject(new Error("get_resource_details not stubbed")),
  ),
  getCachedScan: vi.fn(async (): Promise<CachedScan | null> => null),
  cancelScan: vi.fn(async (): Promise<void> => {}),
  onScanStarted: listen<void>("scanStarted"),
  onScanProviderStarted: listen<string>("scanProviderStarted"),
  onScanProgress: listen<{ provider: string; done: number; total: number }>("scanProgress"),
  onScanCompleted: listen<ScanSummary>("scanCompleted"),

  // ---- cleanup
  previewCleanup: vi.fn((_ids: string[]): Promise<CleanupPreview> =>
    Promise.reject(new Error("preview_cleanup not stubbed")),
  ),
  runCleanup: vi.fn((_ids: string[], _token?: string): Promise<CleanupResult[]> =>
    Promise.reject(new Error("cleanup not stubbed")),
  ),
  cancelCleanup: vi.fn(async (): Promise<void> => {}),
  onCleanupStarted: listen<string[]>("cleanupStarted"),
  onCleanupProgress: listen<{ done: number; total: number }>("cleanupProgress"),
  onCleanupCompleted: listen<CleanupResult[]>("cleanupCompleted"),

  // ---- settings
  getSettings: vi.fn((): Promise<Settings> =>
    Promise.reject(new Error("get_settings not stubbed")),
  ),
  updateSettings: vi.fn((s: Settings): Promise<Settings> => {
    void s;
    return Promise.reject(new Error("update_settings not stubbed"));
  }),
  getProviders: vi.fn((): Promise<ProviderInfo[]> =>
    Promise.reject(new Error("get_providers not stubbed")),
  ),
  detectScanRoots: vi.fn((): Promise<ScanRootCandidate[]> =>
    Promise.reject(new Error("detect_scan_roots not stubbed")),
  ),
  getDiskFree: vi.fn((): Promise<DiskFree> =>
    Promise.reject(new Error("get_disk_free not stubbed")),
  ),

  // ---- history
  getHistoryRuns: vi.fn((_limit?: number): Promise<CleanupRun[]> =>
    Promise.reject(new Error("get_history_runs not stubbed")),
  ),

  // ---- update
  getUpdateStatus: vi.fn((): Promise<UpdateStatus> =>
    Promise.reject(new Error("get_update_status not stubbed")),
  ),
  checkForUpdate: vi.fn((): Promise<UpdateStatus> =>
    Promise.reject(new Error("check_for_update not stubbed")),
  ),
  installUpdate: vi.fn((): Promise<UpdateStatus> =>
    Promise.reject(new Error("install_update not stubbed")),
  ),
  restartApp: vi.fn(async (): Promise<void> => {}),
  onUpdateStatus: listen<UpdateStatus>("updateStatus"),

  // ---- tray
  onTrayAction: listen<string>("trayAction"),
};

/** Forget every recorded callback, every recorded call, and every
 *  `mockResolvedValueOnce`/`mockRejectedValueOnce` queue. The original
 *  "not stubbed" implementations survive `mockReset` (vitest restores the
 *  fn passed to `vi.fn`), so an un-stubbed call still fails loudly. */
export function resetTauriMock() {
  vi.resetAllMocks();
  listeners.scanStarted = undefined;
  listeners.scanProviderStarted = undefined;
  listeners.scanProgress = undefined;
  listeners.scanCompleted = undefined;
  listeners.cleanupStarted = undefined;
  listeners.cleanupProgress = undefined;
  listeners.cleanupCompleted = undefined;
  listeners.updateStatus = undefined;
  listeners.trayAction = undefined;
}
