/**
 * Pure navigation model: which page is showing, how the keyboard maps onto
 * it, and the shapes the tray can ask for. No Svelte, no Tauri — unit-tested
 * with vitest, exactly like `$lib/view` and `$lib/history`.
 *
 * Pages replace the old modal dialogs, so a route id is also the unit the
 * `tray_action` event validates against (an unknown id is refused rather
 * than silently dropped).
 */
import { Boxes, Gauge, History, Settings } from "lucide-svelte";
import type { IconComponent } from "./icons";

export type RouteId = "overview" | "resources" | "history" | "settings";

export interface RouteDef {
  id: RouteId;
  label: string;
  /** One line under the page title; says what the page is for. */
  hint: string;
  /** Position key ⌘1…⌘4 binds to. */
  digit: number;
  icon: IconComponent;
}

export const ROUTES: RouteDef[] = [
  {
    id: "overview",
    label: "Overview",
    hint: "What is reclaimable, and what changed since the last scan",
    digit: 1,
    icon: Gauge,
  },
  {
    id: "resources",
    label: "Resources",
    hint: "Search, select and clean what the providers found",
    digit: 2,
    icon: Boxes,
  },
  {
    id: "history",
    label: "History",
    hint: "Every cleanup run, with what was requested and what was freed",
    digit: 3,
    icon: History,
  },
  {
    id: "settings",
    label: "Settings",
    hint: "What gets scanned, what may be deleted, and when",
    digit: 4,
    icon: Settings,
  },
];

export const DEFAULT_ROUTE: RouteId = "overview";

const ROUTE_IDS = new Set<string>(ROUTES.map((r) => r.id));

export function isRouteId(x: unknown): x is RouteId {
  return typeof x === "string" && ROUTE_IDS.has(x);
}

export function routeDef(id: RouteId): RouteDef {
  const found = ROUTES.find((r) => r.id === id);
  if (!found) throw new Error(`unknown route: ${id}`);
  return found;
}

/** ⌘1…⌘4 → route. Anything else is not a navigation shortcut. */
export function routeByDigit(digit: number): RouteId | null {
  return ROUTES.find((r) => r.digit === digit)?.id ?? null;
}

export type SettingsTab = "scan" | "cleanup" | "appearance" | "advanced";

export interface SettingsTabDef {
  id: SettingsTab;
  label: string;
  hint: string;
}

export const SETTINGS_TABS: SettingsTabDef[] = [
  { id: "scan", label: "Scan", hint: "What gets looked at, and what is skipped" },
  { id: "cleanup", label: "Cleanup", hint: "What may be deleted, and when" },
  { id: "appearance", label: "Appearance", hint: "Theme and row display" },
  { id: "advanced", label: "Advanced", hint: "Resets and details" },
];

export function settingsTab(id: SettingsTab): SettingsTabDef {
  const found = SETTINGS_TABS.find((t) => t.id === id);
  if (!found) throw new Error(`unknown settings tab: ${id}`);
  return found;
}

export type HotkeyAction =
  | { kind: "route"; route: RouteId }
  | { kind: "scan" }
  | { kind: "settings" }
  | { kind: "focus-search" };

/** Only what `resolveHotkey` needs to look at, so it stays testable. */
export interface HotkeyEvent {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  target?: EventTarget | null;
}

const TEXT_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT"]);

/**
 * A keystroke aimed at a text field belongs to that field: typing "2" into a
 * scan root must never jump to the Resources page.
 */
export function isTextEntry(target: HotkeyEvent["target"]): boolean {
  if (!target) return false;
  const el = target as { tagName?: string; isContentEditable?: boolean };
  if (el.isContentEditable) return true;
  return TEXT_TAGS.has((el.tagName ?? "").toUpperCase());
}

/**
 * Resolve a keystroke to a shell action, or null when the shell must stay out
 * of the way. Only the "command" modifier counts — ⌘ on macOS, Ctrl elsewhere.
 */
export function resolveHotkey(e: HotkeyEvent): HotkeyAction | null {
  if (isTextEntry(e.target)) return null;
  if (e.shiftKey || e.altKey) return null;
  if (!e.metaKey && !e.ctrlKey) return null;

  if (/^[1-9]$/.test(e.key)) {
    const route = routeByDigit(Number(e.key));
    return route ? { kind: "route", route } : null;
  }
  if (e.key === "r" || e.key === "R") return { kind: "scan" };
  if (e.key === ",") return { kind: "settings" };
  if (e.key === "f" || e.key === "F") return { kind: "focus-search" };
  return null;
}