/**
 * Navigation store (Svelte 5 runes): the visible page and its Settings tab.
 * Pure matching lives in `$lib/nav` so it stays unit-testable.
 *
 * Session-only on purpose — a page id is never sent to the backend, so a
 * reload always starts on Overview (see `DEFAULT_ROUTE`).
 */
import { DEFAULT_ROUTE, type RouteId, type SettingsTab } from "$lib/nav";

class NavStore {
  route = $state<RouteId>(DEFAULT_ROUTE);
  settingsTab = $state<SettingsTab>("scan");
  /** Set by ⌘F; the Resources toolbar consumes it once it is mounted, so a
   * later visit to that page never steals focus. */
  pendingSearchFocus = $state(false);
  /** Per-page scroll offset, so returning to a long list keeps its place. */
  scroll = $state<Record<RouteId, number>>({
    overview: 0,
    resources: 0,
    history: 0,
    settings: 0,
  });

  navigate(route: RouteId) {
    this.route = route;
  }

  /** ⌘F from anywhere: land on the list, then let it take the focus. */
  focusSearch() {
    this.route = "resources";
    this.pendingSearchFocus = true;
  }

  /** Route change that also asks the caller to act (tray "Review safe…"). */
  open(route: RouteId, tab?: SettingsTab) {
    this.route = route;
    if (tab) this.settingsTab = tab;
  }

  is(route: RouteId) {
    return this.route === route;
  }

  saveScroll(route: RouteId, y: number) {
    this.scroll[route] = y;
  }

  resetScroll(route: RouteId) {
    this.scroll[route] = 0;
  }
}

export const navStore = new NavStore();