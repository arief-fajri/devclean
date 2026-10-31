/**
 * Per-provider display metadata: icon, colours, and a short label used by
 * the scan progress list. The backend decides safety; this file only
 * decides how a provider *looks* and how it is named. Unknown ids fall
 * back to a generic drive.
 */
import {
  Apple,
  Archive,
  Bird,
  Boxes,
  BrainCircuit,
  Coffee,
  Container,
  Database,
  FileText,
  Gem,
  Globe,
  HardDrive,
  MessageSquare,
  Monitor,
  Package,
  Smartphone,
  Terminal,
  Trash2,
} from "lucide-svelte";
import type { IconComponent } from "./icons";

export interface ProviderMeta {
  icon: IconComponent;
  /** Small check shown next to a provider that finished scanning. */
  tick: IconComponent;
  /** Short name for tight spaces (the scan progress list). */
  label: string;
  /** Icon color (light + dark). */
  accent: string;
  /** 40px tile background (light + dark). */
  tile: string;
}

const FALLBACK: ProviderMeta = {
  icon: HardDrive,
  tick: Archive,
  label: "Other",
  accent: "text-zinc-500 dark:text-zinc-400",
  tile: "bg-zinc-100 dark:bg-zinc-800",
};

export const PROVIDER_META: Record<string, ProviderMeta> = {
  node_modules: {
    icon: Package,
    tick: Package,
    label: "node_modules",
    accent: "text-lime-600 dark:text-lime-400",
    tile: "bg-lime-100 dark:bg-lime-950",
  },
  rust_target: {
    icon: Boxes,
    tick: Boxes,
    label: "Rust target",
    accent: "text-orange-600 dark:text-orange-400",
    tile: "bg-orange-100 dark:bg-orange-950",
  },
  go_cache: {
    icon: Coffee,
    tick: Coffee,
    label: "Go cache",
    accent: "text-cyan-600 dark:text-cyan-400",
    tile: "bg-cyan-100 dark:bg-cyan-950",
  },
  docker: {
    icon: Container,
    tick: Container,
    label: "Docker build cache",
    accent: "text-sky-600 dark:text-sky-400",
    tile: "bg-sky-100 dark:bg-sky-950",
  },
  docker_pruned: {
    icon: Trash2,
    tick: Trash2,
    label: "Docker images & volumes",
    accent: "text-red-600 dark:text-red-400",
    tile: "bg-red-100 dark:bg-red-950",
  },
  npm_cache: {
    icon: Package,
    tick: Package,
    label: "npm/pnpm/yarn",
    accent: "text-red-500 dark:text-red-400",
    tile: "bg-red-100 dark:bg-red-950",
  },
  python_cache: {
    icon: Terminal,
    tick: Terminal,
    label: "Python caches",
    accent: "text-blue-600 dark:text-blue-400",
    tile: "bg-blue-100 dark:bg-blue-950",
  },
  cargo_registry: {
    icon: Archive,
    tick: Archive,
    label: "Cargo registry",
    accent: "text-amber-600 dark:text-amber-400",
    tile: "bg-amber-100 dark:bg-amber-950",
  },
  gradle_cache: {
    icon: Database,
    tick: Database,
    label: "Gradle caches",
    accent: "text-emerald-600 dark:text-emerald-400",
    tile: "bg-emerald-100 dark:bg-emerald-950",
  },
  xcode_data: {
    icon: Apple,
    tick: Apple,
    label: "Xcode data",
    accent: "text-zinc-500 dark:text-zinc-400",
    tile: "bg-zinc-100 dark:bg-zinc-800",
  },
  global_cache: {
    icon: HardDrive,
    tick: HardDrive,
    label: "System caches",
    accent: "text-teal-600 dark:text-teal-400",
    tile: "bg-teal-100 dark:bg-teal-950",
  },
  opencode_fs: {
    icon: FileText,
    tick: FileText,
    label: "opencode data",
    accent: "text-indigo-600 dark:text-indigo-400",
    tile: "bg-indigo-100 dark:bg-indigo-950",
  },
  opencode_db: {
    icon: MessageSquare,
    tick: MessageSquare,
    label: "opencode history",
    accent: "text-purple-600 dark:text-purple-400",
    tile: "bg-purple-100 dark:bg-purple-950",
  },
  claude: {
    icon: MessageSquare,
    tick: MessageSquare,
    label: "Claude data",
    accent: "text-orange-600 dark:text-orange-400",
    tile: "bg-orange-100 dark:bg-orange-950",
  },
  go_vendor: {
    icon: Coffee,
    tick: Coffee,
    label: "Go vendor",
    accent: "text-teal-700 dark:text-teal-300",
    tile: "bg-teal-100 dark:bg-teal-950",
  },
  flutter: {
    icon: Bird,
    tick: Bird,
    label: "Flutter",
    accent: "text-blue-500 dark:text-blue-300",
    tile: "bg-blue-100 dark:bg-blue-950",
  },
  android: {
    icon: Smartphone,
    tick: Smartphone,
    label: "Android build",
    accent: "text-green-600 dark:text-green-400",
    tile: "bg-green-100 dark:bg-green-950",
  },
  pods: {
    icon: Gem,
    tick: Gem,
    label: "CocoaPods",
    accent: "text-rose-600 dark:text-rose-400",
    tile: "bg-rose-100 dark:bg-rose-950",
  },
  browser_temp: {
    icon: Globe,
    tick: Globe,
    label: "Browser temp",
    accent: "text-fuchsia-600 dark:text-fuchsia-400",
    tile: "bg-fuchsia-100 dark:bg-fuchsia-950",
  },
  llm_models: {
    icon: BrainCircuit,
    tick: BrainCircuit,
    label: "LLM models",
    accent: "text-violet-600 dark:text-violet-400",
    tile: "bg-violet-100 dark:bg-violet-950",
  },
  ruby: {
    icon: Coffee,
    tick: Coffee,
    label: "Ruby caches",
    accent: "text-red-700 dark:text-red-300",
    tile: "bg-red-100 dark:bg-red-950",
  },
  wails: {
    icon: Monitor,
    tick: Monitor,
    label: "Wails build output",
    accent: "text-cyan-700 dark:text-cyan-300",
    tile: "bg-cyan-100 dark:bg-cyan-950",
  },
};

export function providerMeta(id: string): ProviderMeta {
  return PROVIDER_META[id] ?? FALLBACK;
}
