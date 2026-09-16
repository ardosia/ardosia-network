# Next Work

1. Reconcile `cleanup/runtime-hardening-clean` with `main` through a normal PR if that maintenance line remains canonical.
   - The branch is ahead of `main` and contains runtime hardening plus maintenance state.
   - Do not silently move `ardosia-server` off its pinned `f71da57...` revision.
   - Review the functional diff and run the full standalone network gate before merging runtime changes.

2. After any network/runtime merge, run the complete Rust 1.98 gate documented in `CONTEXT.md` and update `STATE.md` with actual PASS/FAIL/BLOCKED/NOT RUN results.

3. Treat performance work separately.
   - Historical load reports are centralized in `ardosia-docs` and remain evidence only.
   - Any new performance claim requires an explicit workload, environment, baseline, and measured result.

Completed prerequisite: Network durable overview/benchmark documentation has been centrally migrated and verified; repository-local historical benchmark duplicates have been removed from both live lines.
