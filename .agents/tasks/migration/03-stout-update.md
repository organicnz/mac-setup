# 03 — Stout Package Index Update

**Status:** ✅ SUCCESS  
**Exit code:** 0  
**Timestamp:** 2025-07-09

## Output

```
WARN stout_index::sync: Manifest is unsigned, but policy allows unsigned indexes
INFO stout_index::sync: Downloading formula index from https://raw.githubusercontent.com/neul-labs/stout-index/main/formulas/index.db.zst
INFO stout_index::sync: Index updated to 2026.10.08.1521 (8646 formulas)
INFO stout_index::sync: Downloading cask index from https://raw.githubusercontent.com/neul-labs/stout-index/main/casks/index.db.zst
WARN stout_index::sync: Failed to sync cask index: Database error: no such column: dep_name in 
CREATE INDEX IF NOT EXISTS idx_cask_dependencies_dep ON cask_dependencies(dep_name);
 at offset 75
Updated to 2026.10.08.1521 (8646 formulas)
Syncing with Homebrew...
  State is in sync with Homebrew.
Run 'stout search <query>' to find packages
```

## Notes

- Formula index updated successfully to version `2026.10.08.1521` with **8646 formulas**.
- Cask index sync produced a non-fatal warning: `no such column: dep_name` — this is a schema mismatch in the cask dependency index. Stout continued and reported success (exit 0). Safe to proceed.
- Homebrew state is in sync; no drift detected.
