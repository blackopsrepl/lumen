# AGENTS.md

Single-binary Rust/Axum service that supervises per-agent Chromium browsers and serves a live viewer. Node exists only for Playwright E2E tests and the container healthcheck.

## Verification gates

CI (`.forgejo/workflows/ci.yml`) runs, in order:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
make ui-test
```

Keep `Cargo.lock` in sync — CI builds `--locked`. Run the same order locally before committing, or just `make ci`, which mirrors it.

- Single Rust test: `cargo test host_matching_ignores` (name substring).
- Single E2E test: set the env from `make ui-test`, then `npm run test:e2e -- -g "pattern"`.
- Dependency audit: `npm audit --omit=dev`. There is no `cargo-audit`/`cargo-deny` on this host.

## The 8899 port trap

A production `lumen` container usually runs on 8899 with host networking and live sessions. `playwright.config.js` uses `reuseExistingServer: true`, so if the suite targets a port where anything answers, it silently tests that process — not your worktree.

- Always run E2E via `make ui-test`: it uses `LUMEN_TEST_PORT` (default 18899) and self-hosts a disposable service with `tests/e2e/lumen.toml`.
- Only test the production service by explicitly setting `LUMEN_URL`.
- Do not restart the production container to "refresh" tests; it owns real sessions. `bin/smoke.sh` runs against the live service and honors `LUMEN_PORT`/`LUMEN_CONTAINER`.

## Config and deployment facts

- Config layering in `src/config.rs::Config::load`: `LUMEN_CONFIG` file (default `config/lumen.toml`), then `LUMEN_PORT` / `LUMEN_CHROME` env overrides.
- `compose.yaml` must forward any config surface the service reads (`LUMEN_PORT`, `LUMEN_CHROME`) and the healthcheck must probe the same port — these were once out of sync. The container's log level is `LUMEN_LOG`; never interpolate the host's `RUST_LOG`, which leaks in from the operator's shell.
- `bin/up.sh` converges: it recreates the container only when the running image's `org.opencontainers.image.revision` label differs from the checkout's, because podman-compose otherwise keeps an old container serving a stale binary. Image ids cannot be compared directly — every rebuild produces a new one.
- Container runs with `network_mode: host`: pages reach host dev servers at `http://127.0.0.1:<port>`; `host.containers.internal` / `host.docker.internal` are mapped to loopback via `extra_hosts`.
- `bin/common.sh` sources `.env` and wraps `lumen`/`podman` helpers; host helper scripts honor `LUMEN_PORT`.

## Code invariants

- `Policy::check` (`src/config.rs`) is the single navigation-policy predicate, enforced at two points: Lumen's HTTP preflight (403) and a per-tab CDP `Fetch.enable` interception (`ERR_BLOCKED_BY_CLIENT`). Never add a second check. Enforcement scope (covers every tab Lumen mediates; not a sandbox for agent-created tabs) is documented in README "Security" — read it before reasoning about "bypass".
- Exactly one managed page per `CdpSession` (`src/cdp.rs`); tab activation replaces it and stops the old screencast. Activation happens through the API and through the supervisor watcher that adopts tabs the browser opens itself (`target=_blank`, popups). `ViewHub::rebind` must follow every managed-page swap, wherever it happens.
- Browser profiles are ephemeral and owned by one instance: `<data_dir>/run/<name>-<suffix>` exists only while that browser is alive. Shutdown, reaping, and startup reconciliation remove them; the path is service-generated so deletion never derives from API input.
- `chromiumoxide::Page::close(self)` consumes the page — clone `target_id` before closing if you need it afterwards.
- UI assets are embedded via `rust-embed` (`src/http.rs`): debug builds read `ui/` from disk, release builds embed. Verify UI changes under `cargo run`, rebuild the image for release behavior.

## Conventions

- Conventional commit subjects with scope: `fix(cdp): …`, `test(viewer): …`, `docs(policy): …`. No changelog tooling in this repo.
- Do not push or publish unless asked. Two remotes: `origin` is the local Forgejo (`http://vigilance:3002/blackopsrepl/lumen.git`) and `blackopsrepl` is GitHub. Both run the same gates, and `.github/workflows/ci.yml` declares `workflow_call` so `.github/workflows/release.yml` reuses it — a pushed `v*` tag publishes a GitHub Release with generated notes, gated on those checks.
