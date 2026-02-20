# Electron Installer Plan (acs-0001)

## Overview

Create an Electron app that bundles the ACS daemon binary and the Next.js frontend, providing a one-click installer and GUI for the ACS system.

## Architecture

```
agent-cron-scheduler/
├── acs/                            # Rust daemon (existing, unchanged)
├── electron/                       # NEW - Electron app (npm workspaces monorepo)
│   ├── package.json                # Root: workspaces, version, convenience scripts
│   └── packages/
│       ├── app/                    # Electron shell
│       │   ├── electron/
│       │   │   ├── main.js         # Main process: start daemon, create window
│       │   │   └── preload.js      # IPC bridge: port polling, service mgmt
│       │   ├── package.json        # Electron deps
│       │   └── electron-builder.yml
│       └── frontend/               # MOVED from root frontend/
│           ├── src/                # Existing Next.js dashboard code
│           ├── package.json
│           ├── next.config.ts
│           └── ...
├── .github/workflows/
│   └── release.yml                 # CI/CD for Win/Mac/Linux
├── release.sh                      # Version bump (bash)
└── release.ps1                     # Version bump (powershell)
```

## Runtime Flow

1. User runs installer → Electron app + ACS binary installed
2. Electron launches → spawns `acs start` (auto-registers as OS service)
3. Preload polls `{dataDir}/acs.port` every 2.5s
4. Port found → frontend loads at `http://localhost:{port}`
5. Daemon offline → shows "offline" banner, auto-retries every 2.5s

## Steps

### Step 1: Move frontend into electron/packages/frontend [x]
- Move `frontend/` → `electron/packages/frontend/`
- Keep all existing code, deps, config intact
- Delete old `frontend/` directory
- Leave `acs/build.rs` and `acs/web/` as-is (swagger docs)

### Step 2: Create electron root package.json [x]
- `electron/package.json` with npm workspaces (`packages/*`)
- Convenience scripts: `dev`, `dev:electron`, `dist:win`, `dist:mac`, `dist:linux`
- Version `0.1.0`

### Step 3: Create Electron app package [x]
- `electron/packages/app/package.json` — Electron 33.x, electron-builder ^25
- Scripts: `dev`, `dist:win`, `dist:mac`, `dist:linux`

### Step 4: Electron main process [x]
- `electron/packages/app/electron/main.js`
- `getAcsBinaryPath()`: dev → `../../../acs/target/release/acs`, packaged → `process.resourcesPath/acs-binary/acs`
- `getDataDir()`: Windows `%LOCALAPPDATA%/agent-cron-scheduler`, macOS `~/Library/Application Support/agent-cron-scheduler`, Linux `~/.local/share/agent-cron-scheduler`
- `startDaemon()`: health check first, spawn if not running (detached, windowsHide)
- `createWindow()`: 1200x800, dev → localhost:3000, prod → out/index.html

### Step 5: Preload script [x]
- `electron/packages/app/electron/preload.js`
- Expose `window.acs` via contextBridge
- `getDaemonPort()`, `getDaemonUrl()`, `getDataDir()`
- `installService()`, `uninstallService()`, `getServiceStatus()`
- Port file polling support

### Step 6: Frontend API URL tweak [x]
- Modify `electron/packages/frontend/src/lib/api.ts`
- Check `window.acs?.getDaemonUrl?.()` first, fallback to env var, then empty string

### Step 7: Connection status awareness [x]
- Add `useConnectionStatus()` hook to frontend
- Offline banner in root layout
- Poll `/health` every 2.5s, auto-reconnect

### Step 8: electron-builder config [x]
- `electron/packages/app/electron-builder.yml`
- Bundle: electron shell + out/ (frontend) + ACS binary (extraResources)
- Targets: NSIS (win), DMG (mac), AppImage (linux)

### Step 9: GitHub Actions release workflow [x]
- `.github/workflows/release.yml`
- Trigger: push tags `v*`
- Matrix: windows-latest, macos-latest, ubuntu-latest
- Steps: checkout → setup node 20 → setup rust → build frontend → copy out/ → build rust → package electron → upload artifacts
- Release job: download artifacts → create GitHub release with download table

### Step 10: Release scripts [x]
- `release.sh` and `release.ps1`
- Bump version in electron package.json files, commit, tag, push

## Key References

- **webpilot project**: `C:\Users\J\Documents\Github\webpilot` — reference for Electron setup, electron-builder config, GitHub Actions, release scripts
- **ACS daemon port file**: `acs/src/daemon/mod.rs:912-915` — writes `{data_dir}/acs.port`
- **ACS data dir resolution**: `acs/src/daemon/mod.rs:316-349`
- **ACS service registration**: `acs/src/daemon/service.rs` — Windows Task Scheduler, macOS launchd, Linux systemd
- **Frontend API client**: `frontend/src/lib/api.ts` — BASE_URL detection
