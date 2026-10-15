# ADR: `size_bytes` is allocated blocks, not logical length

**Status:** accepted (v0.8.0)
**Supersedes:** the implicit choice to sum `Metadata::len()`

## Context

Until v0.8.0 every size in DevClean was computed by summing `m.len()` over the
files in a tree. `len()` is the **logical** size — the number a file reports,
not the space it occupies. Measuring three real `node_modules` trees against
`du` showed the two disagree, and **not in a consistent direction**:

| Tree | DevClean (logical) | `du` (allocated) | Delta |
|---|---|---|---|
| A Svelte app's `node_modules` | 0.95 GB | 1.17 GB | **−19%** |
| A Node service's `node_modules` | 1.13 GB | 1.15 GB | −2% |
| A monorepo frontend's `node_modules` | 0.79 GB | 0.78 GB | +1% |

Two independent effects were cancelling each other out:

### Effect 1 — block rounding under-reports (tiny files)

A 1-byte file occupies a whole 4 KiB block:

```text
$ stat -f "blocks=%b size=%z" one-byte
blocks=8 size=1          →  8 × 512 = 4096 bytes allocated for 1 byte of payload
```

A tree full of small files (the first row above) therefore has far more
allocated bytes than its payload sum. This is the −19%.

### Effect 2 — hard links over-report (duplicated inodes)

`len()` is summed once per directory entry. A hard link is a second *name* for
the same inode, so it was counted a second time. Measured on this repository's
own Rust `target/`:

```text
du -sk target           10.78 GB   ← the truth
logical sum             26.33 GB
allocated, no dedup     26.63 GB
files                   144,828
hardlinked (nlink > 1)  132,897    (92%)
double-counted bytes    15.62 GB
```

Cargo hard-links build artifacts between `target/debug/deps/` and
`target/debug/`, so DevClean was reporting **15.6 GB that deleting the
directory would not free**. This is the effect that made the delta positive
where it appeared at all.

## Decision

`Resource.size_bytes` is **allocated blocks** — what deleting the resource
actually frees.

1. **Allocated** = `Σ st_blocks × 512` over **files only**.
   Directories are excluded on purpose: they contribute real blocks on ext4
   and zero on APFS, and excluding them keeps `allocated == 0` meaning
   "genuinely empty" on every platform. Providers rely on that to skip empty
   caches (`if size == 0 { return None }`), and seven of them do.

2. **Hard links are counted once per artifact**, keyed by `(dev, ino)`. The
   dedup is scoped to the artifact: an inode shared *between* two artifacts is
   still counted in both, because each one genuinely frees those blocks when
   deleted alone. Netting that out is a total-level concern, deferred to the
   cross-artifact deduplication in the [roadmap](../development/roadmap.md).

3. **`apparent_bytes` is reported separately**, and only when it is materially
   *larger* than allocated — the sparse-file case (a 460 GB-declared disk image
   occupying 8 GB). Dense trees where block rounding makes allocated larger
   return `None`; annotating them would put a meaningless "appears as" note on
   every ordinary directory.

4. **`dir_size` keeps returning a single `u64`** (the allocated figure), so the
   seven `size == 0` predicates did not change meaning and did not need
   rewriting. Only providers that *build* a `Resource` moved to the
   detail-returning variant.

## Consequences

- `CACHE_VERSION` 1 → 2. A v1 cache holds logical sizes up to 19% low and is
  rejected rather than displayed. A dashboard that quietly disagrees with `du`
  is worse than a rescan.
- `Resource.apparent_bytes` is a new optional contract field, additive.
- Reported totals fall on any tree with hard links. On this repository's own
  Rust `target/`, 15.6 GB of the old figure was the hard-link double count, so
  the number dropping by that much is the figure becoming true, **not** a
  regression.
- Verified `delta = +0.0%` against `du` on six `node_modules` trees.

## Rejected alternatives

- **Keep logical size and document the discrepancy.** Rejected: the number on
  the button says "reclaimable", so it has to be the number that gets
  reclaimed.
- **Use allocated everywhere including cross-artifact.** Rejected for now: it
  would make a `node_modules` that is 100% hard-linked into a pnpm store read
  as `0` and vanish from the list. Per-artifact figures stay honest; only the
  total deduplicates.
- **Count directory blocks too.** Rejected: platform-dependent, and it breaks
  `allocated == 0 ⟺ empty`.
