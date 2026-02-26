# Agent Instructions

## Project Structure

This is a monorepo with three main components:

- **`acs/`** -- Rust backend (cron scheduling daemon). The binary is `acs`. Run `cargo build`, `cargo test`, and `cargo clippy` from within the `acs/` directory.
- **`acs/frontend/`** -- Next.js 16 frontend (static export via `output: "export"`). Built with `npm run build` and embedded into the Rust binary via `rust-embed`.
- **`electron/`** -- Electron wrapper that packages the app as a desktop installer. Build with `npm run build` from the `electron/` directory.
- **`docs/`** -- Project documentation. See `docs/INDEX.md` for an overview.

### Key Commands

```bash
# Rust backend (run from acs/)
cargo build              # Build the daemon
cargo test               # Run all tests (~285 total)
cargo clippy             # Lint

# Frontend (run from acs/frontend/)
npm run build            # Static export to acs/frontend/out/

# Electron (run from electron/)
npm run build            # Build the Electron installer
```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
4. **Clean up** - Clear stashes, prune remote branches
5. **Verify** - All changes committed AND pushed
6. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

