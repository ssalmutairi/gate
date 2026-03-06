# Gate — Project Instructions

## Pre-commit Checklist (for version bumps / releases)

Before committing a version change, update ALL of these:

1. **`Cargo.toml`** (workspace) — `version` field
2. **`README.md`** — version badge and install example (`VERSION=v...`)
3. **`CHANGELOG.md`** — add new version entry under `[Unreleased]`
4. **`deploy/kubernetes/admin-deployment.yaml`** — container image tag
5. **`deploy/kubernetes/proxy-deployment.yaml`** — container image tag
6. **`charts/gate/Chart.yaml`** — `appVersion` field
7. Run `cargo test -p standalone` to verify tests pass

## Post-push Checklist (for releases)

After pushing to main:

1. Create and push a git tag: `git tag v<VERSION> && git push origin v<VERSION>`
2. Verify the Release workflow started: `gh run list --limit 1`
3. Monitor until all jobs pass: `gh run view <RUN_ID>`

## Project Defaults

- **Standalone defaults**: `DATABASE_URL=sqlite://gate.db`, `ADMIN_TOKEN=changeme`
- **Rust version**: 1.86+
- **Test commands**:
  - `cargo test -p standalone` — 27 tests, no dependencies
  - `cargo test -p shared` — 7 tests, no dependencies
  - `cargo test -p admin` — requires PostgreSQL
  - `cargo test -p proxy` — requires PostgreSQL
  - `cd dashboard && npm test` — 45 tests

## File Conventions

- Standalone crate: `crates/standalone/` — self-contained, does NOT modify admin/proxy/shared crates
- SQLite migrations: `crates/standalone/migrations/`
- PostgreSQL migrations: `migrations/`
- Dashboard: `dashboard/` (React + Vite + TailwindCSS)
