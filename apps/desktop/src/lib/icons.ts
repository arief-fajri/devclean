/**
 * A lucide-svelte icon component. lucide types its icons as legacy
 * `SvelteComponentTyped` subclasses carrying `IconProps`, which neither
 * the Svelte 5 `Component` interface nor the legacy base constructor
 * accept under strict variance — so this stays `any`. Display-only leaf
 * type; the icons themselves are still fully checked at their use sites
 * (`<Icon size={…} class={…} />`).
 */

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type IconComponent = any;
