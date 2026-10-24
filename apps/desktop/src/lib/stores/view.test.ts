import { beforeEach, describe, expect, it } from "vitest";
import { viewStore } from "./view.svelte";
import { DEFAULT_VIEW } from "$lib/view";

/**
 * View store: global search / filter / sort state plus the session-only set
 * of expanded provider ids.
 *
 * The matching and sorting logic lives in `$lib/view` and is tested there;
 * what matters here is that the store mirrors its options faithfully, that
 * `active` lights up the "clear filters" affordance exactly when something
 * is narrowing, and that a provider whose last item was cleaned does not
 * leave a ghost expansion behind.
 */
describe("viewStore", () => {
  beforeEach(() => {
    viewStore.reset();
    viewStore.expanded = [];
  });

  it("starts at the default view, with nothing active", () => {
    expect(viewStore.options).toEqual(DEFAULT_VIEW);
    expect(viewStore.active).toBe(false);
  });

  it("options reflects live state", () => {
    viewStore.query = "node_modules";
    viewStore.safety = "safe";
    viewStore.sort = "largest";
    viewStore.showPaths = true;
    expect(viewStore.options.query).toBe("node_modules");
    expect(viewStore.options.safety).toBe("safe");
    expect(viewStore.options.sort).toBe("largest");
    expect(viewStore.options.showPaths).toBe(true);
  });

  it("active is true for any narrowing and false for a bare sort change", () => {
    viewStore.sort = "largest";
    expect(viewStore.active).toBe(false);

    viewStore.query = " ";
    // Whitespace-only query must not count as a filter.
    expect(viewStore.active).toBe(false);
    viewStore.query = "x";
    expect(viewStore.active).toBe(true);

    viewStore.reset();
    viewStore.safety = "review";
    expect(viewStore.active).toBe(true);

    viewStore.reset();
    viewStore.staleOnly = true;
    expect(viewStore.active).toBe(true);

    viewStore.reset();
    viewStore.selectedOnly = true;
    expect(viewStore.active).toBe(true);
  });

  it("reset restores every field to the default", () => {
    viewStore.query = "x";
    viewStore.safety = "review";
    viewStore.staleOnly = true;
    viewStore.selectedOnly = true;
    viewStore.sort = "name";
    viewStore.showPaths = true;
    viewStore.reset();
    expect(viewStore.options).toEqual(DEFAULT_VIEW);
    expect(viewStore.active).toBe(false);
  });

  it("toggleExpand adds and removes exactly the named provider", () => {
    viewStore.toggleExpand("docker");
    viewStore.toggleExpand("node_modules");
    expect(viewStore.expanded).toEqual(["docker", "node_modules"]);

    viewStore.toggleExpand("docker");
    expect(viewStore.expanded).toEqual(["node_modules"]);
  });

  it("pruneExpanded drops providers that no longer exist", () => {
    viewStore.expanded = ["docker", "node_modules", "go_cache"];
    viewStore.pruneExpanded(["docker", "go_cache"]);
    expect(viewStore.expanded).toEqual(["docker", "go_cache"]);
  });

  it("pruneExpanded leaves state untouched when nothing vanished", () => {
    viewStore.expanded = ["docker"];
    viewStore.pruneExpanded(["docker", "node_modules"]);
    expect(viewStore.expanded).toEqual(["docker"]);
  });
});
