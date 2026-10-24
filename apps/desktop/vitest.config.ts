/**
 * Vitest config: the Vite config (Svelte plugin, `$lib` alias, Tailwind) plus
 * the test environment.
 *
 * Component tests need a DOM, so the environment is happy-dom for the whole
 * suite — the pure-logic tests simply never touch `window`. `happy-dom`
 * (rather than jsdom) is a deliberate pick: it is faster, maintained, and
 * portable to the Ubuntu CI runner, which is the acceptance for the DOM rig
 * (Wave E2).
 */
import { fileURLToPath, URL } from "node:url";
import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config.ts";

export default mergeConfig(
  viteConfig,
  defineConfig({
    resolve: {
      alias: {
        $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
      },
      // Node resolves the `svelte` package to its SERVER entry by default
      // (`"default": "./src/index-server.js"`), where `mount()` throws
      // "`mount(...)` is not available on the server". Component tests need
      // the client runtime, so the `browser` export condition wins here.
      conditions: ["browser"],
    },
    test: {
      environment: "happy-dom",
      setupFiles: ["./src/lib/test-setup.ts"],
      server: {
        deps: {
          // Let Vite resolve (rather than native node), so the conditions
          // above actually apply to `svelte` and its internal runtime.
          inline: [/\/svelte\//, /^svelte$/, /@testing-library/],
        },
      },
    },
  }),
);
