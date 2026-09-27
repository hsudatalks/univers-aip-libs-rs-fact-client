# FactStore client helpers

`univers-aip-lib-fact-client@0.1.0` provides retry-safe assertion and read projection helpers for a caller-selected `FactStore` Port. The crate keeps the existing API: `assert_fact_for_organization`, `FactAssertOutcome::{Created, Unchanged}` with `fact()` and `receipt()`, `list_fact_values`, `latest_fact_value`, and `fact_value_at`.

The assertion helper checks the requested organization scope, appends a new assertion only when the value changes, and reports same-value writes as `Skipped` replay receipts. Assertion identity is deterministic from the caller's idempotency key. Latest projection ranks every matching result by `asserted_at`, regardless of caller limit; temporal projection honors the FactStore query's half-open validity window.

This is a pure Port helper. It does not implement `FactStore`, persist data, choose a World, authorize organization access, or own Fact/World authority. The selected Port remains responsible for scope enforcement and durable atomic writes; receipt and replay guarantees depend on that Port.

The package pins the C0 Data and World contracts to `1.0.0-rc.1` with default features disabled and only the required core/evidence/operating features enabled. Run `bash scripts/check.sh`, `bash scripts/build.sh`, `bash scripts/package.sh`, and `bash scripts/publish.sh [--dry-run]` in this independent repository. Git hooks run formatting on commit and the full package check on push.
