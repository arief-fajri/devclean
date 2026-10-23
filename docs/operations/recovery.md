# Recovering from a cleanup

DevClean moves things to the Trash by default, so a wrong click is recoverable. This page is the procedure for each way a cleanup can go wrong.

## "I cleaned something I did not mean to"

Nothing is lost. Cleanups move to the Trash unless you turned that off.

### From Finder

1. Open the Trash.
2. Find the folder by name — DevClean puts it in under its own name, so `node_modules` from `~/Projects/storefront` is simply `node_modules`.
3. Drag it back to where it came from.

> **If two items shared a name**, the second is stored as `node_modules.1`, the third as `node_modules.2`, and so on. DevClean never overwrites something already in the Trash, so the first one you moved is still the un-suffixed name. Check which is which by size or by opening it.

> **macOS "Put Back" is not available.** DevClean does not write the Finder metadata that enables it. The item is still there and still recoverable — the one-click undo is what is missing, not the file.

### From the terminal

```bash
# macOS: find it
ls ~/.Trash | grep node_modules

# Move it back
mv ~/.Trash/node_modules ~/Projects/storefront/node_modules
```

```bash
# Linux (XDG): DevClean writes files/ + info/<name>.trashinfo
ls ~/.local/share/Trash/files | grep node_modules

# Move it back
mv ~/.local/share/Trash/files/node_modules ~/Projects/storefront/node_modules

# Then drop the sidecar that pointed at it, so no metadata is left behind
rm ~/.local/share/Trash/info/node_modules.trashinfo
```

> **On Linux the artifact lives in `files/`, not at the top of the Trash.** The sibling `info/<name>.trashinfo` is what makes a file manager show it as trash and restore it; deleting the artifact without deleting its sidecar leaves a ghost entry. `$XDG_DATA_HOME` is honoured when set, so the real path may be `$XDG_DATA_HOME/Trash` instead of `~/.local/share/Trash`.

### If the space has not come back

That is expected. A Trash move relocates the data; it does not free it. The space returns when you empty the Trash:

```bash
# macOS: Finder → Empty Trash
# Linux: empty the Trash in your file manager, or:
rm -rf ~/.local/share/Trash/files/* ~/.local/share/Trash/info/*
```

The cleanup summary says "moved to Trash — space returns when you empty it" for exactly this reason, and the run is recorded in History with `moved_to_trash`.

## "A cleanup failed partway"

Cleanup is per resource. One failure does not stop the rest, and each result carries its own status and reason.

Open **History** and find the run. Each row shows one of:

| Status | What happened | What to do |
|---|---|---|
| `success` | Done. | Nothing. |
| `partial` | Some bytes were freed before an error. | Read the reason; usually a permission problem or a file that vanished mid-run. Rescan and retry. |
| `failed` | Nothing was freed. | The reason is in the row. A `failed` row with `freed_bytes: 0` and a "could not be measured" message means the tool ran but its effect could not be verified — check manually. |
| `cancelled` | You cancelled; the in-flight item finished, the rest were skipped. | Rescan and select the rest. |

A `failed` row never claims bytes it did not measure. If the reason says the freed space could not be measured, treat the cleanup as unverified rather than as having done nothing.

## "DevClean refuses to clean something"

The resource is listed with a badge explaining why. Each reason has one fix:

| Badge | Reason | Fix |
|---|---|---|
| `kept` | Younger than your retention policy. | **Settings → Cleanup → Retention**: raise or clear the age limit. |
| `report only` | AI conversation history, and AI cleanup is off. | **Settings → Cleanup → AI history**: enable it. |
| `needs opt-in` | The action needs privileged operations. | **Settings → Cleanup → Privileged operations**. Read the consequence first — this is the one that removes images and volumes. |
| `scope off` | Docker cleanup is off. | **Settings → Cleanup → Docker**: choose a scope. |
| `protected` | A tracked file in a repository with uncommitted changes. | Commit or stash the changes, then rescan. |
| `browser running` | A browser whose temp files these are is running. | Quit the browser, then rescan. A probe that could not answer also shows this class of caution, without the badge. |

None of these are bypassable from the UI. The backend refuses the resource even if its id is sent directly to the cleanup command, so a badge is an explanation rather than a suggestion.

## "DevClean wants to delete something it should not"

That is a safety bug, and it is treated as P0. Do **not** open a public issue — see [SECURITY.md](../../SECURITY.md).
