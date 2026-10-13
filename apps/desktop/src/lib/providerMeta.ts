/**
 * Per-provider icon + accent. The backend decides safety; this file only
 * decides how a provider *looks*. Unknown ids fall back to a hard drive.
 */
import {
  Apple,
  Archive,
  Boxes,
  Coffee,
  Container,
  Database,
  HardDrive,
  Package,
  Terminal,
} from "lucide-svelte";
import type { IconComponent } from "./icons";

export interface ProviderMeta {
  icon: IconComponent;
  /** Icon color (light + dark). */
  accent: string;
  /** 40px tile background (light + dark). */
  tile: string;
}

const FALLBACK: ProviderMeta = {
  icon: HardDrive,
  accent: "text-zinc-500 dark:text-zinc-400",
  tile: "bg-zinc-100 dark:bg-zinc-800",
};

export const PROVIDER_META: Record<string, ProviderMeta> = {
  node_modules: {
    icon: Package,
    accent: "text-lime-600 dark:text-lime-400",
    tile: "bg-lime-100 dark:bg-lime-950",
  },
  rust_target: {
    icon: Boxes,
    accent: "text-orange-600 dark:text-orange-400",
    tile: "bg-orange-100 dark:bg-orange-950",
  },
  go_cache: {
    icon: Coffee,
    accent: "text-cyan-600 dark:text-cyan-400",
    tile: "bg-cyan-100 dark:bg-cyan-950",
  },
  docker: {
    icon: Container,
    accent: "text-sky-600 dark:text-sky-400",
    tile: "bg-sky-100 dark:bg-sky-950",
  },
  npm_cache: {
    icon: Package,
    accent: "text-red-500 dark:text-red-400",
    tile: "bg-red-100 dark:bg-red-950",
  },
  python_cache: {
    icon: Terminal,
    accent: "text-blue-600 dark:text-blue-400",
    tile: "bg-blue-100 dark:bg-blue-950",
  },
  cargo_registry: {
    icon: Archive,
    accent: "text-amber-600 dark:text-amber-400",
    tile: "bg-amber-100 dark:bg-amber-950",
  },
  gradle_cache: {
    icon: Database,
    accent: "text-emerald-600 dark:text-emerald-400",
    tile: "bg-emerald-100 dark:bg-emerald-950",
  },
  xcode_data: {
    icon: Apple,
    accent: "text-zinc-500 dark:text-zinc-400",
    tile: "bg-zinc-100 dark:bg-zinc-800",
  },
};

export function providerMeta(id: string): ProviderMeta {
  return PROVIDER_META[id] ?? FALLBACK;
}
