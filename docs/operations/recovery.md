# Recovering from a cleanup

Since 1.0.0 every cleanup is permanent: deleted files reclaim space
immediately and cannot be undone. The re-measured preview and the
confirmation checkbox are the safety net — read them before confirming.

Runs recorded before 1.0.0 may still carry `archived in Trash (pre-1.0)`
labels. Those artifacts live in the OS Trash (`~/.Trash` on macOS,
`$XDG_DATA_HOME/Trash/files` on Linux) and can be dragged back by hand;
everything cleaned since 1.0.0 has no Trash copy.

## "I cleaned something I did not mean to"

Regenerate it with the toolchain: `npm install`, `cargo build`, `go build`,
or rebuild the Docker layer. There is no Trash copy to restore for cleanups
done since 1.0.0.

### If the space has not come back

Rescan first — the dashboard re-measures free space after every cleanup, so
a stale number means the scan is old, not that the delete failed. Open
**History** and check the run's `requested → freed` figures: `freed` is
measured `before − after`, never assumed.

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
