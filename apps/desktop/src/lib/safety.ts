/**
 * Display metadata for `SafetyLevel`. The verdict itself always comes from
 * the backend — this file only decides how it *reads*. Status is never
 * color-only: every badge carries a text label (a11y).
 *
 * Display labels differ from wire values on purpose: "Review" reads as an
 * action ("read this first"), while the raw `caution` stays in the contract.
 */
import { OctagonX, ShieldCheck, TriangleAlert } from "lucide-svelte";
import type { IconComponent } from "./icons";
import type { SafetyLevel } from "./tauri";

export interface SafetyMeta {
  label: string;
  badge: string;
  dot: string;
  hint: string;
  icon: IconComponent;
}

export const SAFETY_META: Record<SafetyLevel, SafetyMeta> = {
  safe: {
    label: "Safe",
    badge:
      "bg-emerald-100 text-emerald-800 dark:bg-emerald-900/60 dark:text-emerald-200",
    dot: "bg-emerald-500",
    hint: "Recoverable — can be rebuilt or re-downloaded.",
    icon: ShieldCheck,
  },
  caution: {
    label: "Review",
    badge:
      "bg-amber-100 text-amber-800 dark:bg-amber-900/60 dark:text-amber-200",
    dot: "bg-amber-500",
    hint: "Read the consequence before cleaning.",
    icon: TriangleAlert,
  },
  dangerous: {
    label: "Dangerous",
    badge: "bg-red-100 text-red-800 dark:bg-red-900/60 dark:text-red-200",
    dot: "bg-red-500",
    hint: "High risk — double-check before cleaning.",
    icon: OctagonX,
  },
};

export function safetyMeta(safety: SafetyLevel): SafetyMeta {
  return SAFETY_META[safety];
}
