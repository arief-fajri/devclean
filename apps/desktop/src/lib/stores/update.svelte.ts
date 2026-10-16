/**
 * Update store: the in-app updater state (contract v0.12.0).
 *
 * The backend owns the state machine and broadcasts `update_status`; this
 * store only mirrors it and expresses intent (check / install / restart). It
 * never touches the network itself — every call goes through the typed bridge,
 * and the payload is signature-verified in Rust before it is installed
 * (G-UPD-01).
 */
import {
  checkForUpdate,
  getUpdateStatus,
  installUpdate,
  onUpdateStatus,
  restartApp,
} from "$lib/tauri";
import type { UpdateStatus } from "$lib/tauri";

class UpdateStore {
  /** The last state the backend reported, or null before the first read. */
  status = $state<UpdateStatus | null>(null);
  /** A command is in flight — the controls are disabled while true. */
  busy = $state(false);
  /** The last command's rejection. The backend refuses an install during a
   * scan or cleanup with a reason, and that reason is shown rather than
   * leaving the button silently dead. */
  error = $state<string | null>(null);

  /** Read the current state without a network call, so the Settings page can
   * render immediately — including a check that ran at startup. */
  async refresh() {
    try {
      this.status = await getUpdateStatus();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Mirror the backend state. Fire-and-forget like the other stores. */
  listen() {
    void onUpdateStatus((s) => {
      this.status = s;
    });
  }

  async check() {
    await this.run(() => checkForUpdate());
  }

  async install() {
    await this.run(() => installUpdate());
  }

  async restart() {
    // The process is replaced, so this normally never resolves.
    try {
      await restartApp();
    } catch (e) {
      this.error = String(e);
    }
  }

  private async run(command: () => Promise<UpdateStatus>) {
    if (this.busy) return;
    this.busy = true;
    this.error = null;
    try {
      this.status = await command();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.busy = false;
    }
  }
}

export const updateStore = new UpdateStore();
