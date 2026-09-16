# Next Work

1. Finish the organization-wide documentation migration after `ardosia/ardosia-docs` is created.
   - Copy README/contribution/security prose, historical benchmark reports, and maintenance status documentation with source provenance.
   - Verify central copies before deleting local docs.
   - Keep code, Cargo files, CI, toolchain, license, and `.agent/` locally.

2. Reconcile `cleanup/runtime-hardening-clean` with `main` through a normal PR if that maintenance line remains canonical.
   - The branch is currently ahead of `main` and contains runtime hardening plus status/docs changes.
   - Do not silently move `ardosia-server` off its pinned `f71da57...` revision during cleanup.
   - Review code diff and run the full standalone network gate before merging functional transport changes.

3. After any network/runtime merge, run the complete Rust 1.98 gate documented in `CONTEXT.md` and update `STATE.md` with actual PASS/FAIL/BLOCKED/NOT RUN results.

4. Treat performance work separately.
   - Historical load reports remain evidence only.
   - Any new performance claim requires an explicit workload, environment, baseline, and measured result.
