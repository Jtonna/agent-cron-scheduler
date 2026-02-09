# Beads → GitHub Sync

ACS uses [beads](https://github.com/steveyegge/beads), a git-backed issue tracker, to manage work items locally. Issues live in `.beads/issues.jsonl` alongside the code and are automatically synced to GitHub Issues and the GitHub Projects kanban board when merged to `main`.

## How It Works

```
Local branch                          main branch
┌──────────────┐    PR merge    ┌──────────────────┐    GitHub Action    ┌─────────────────┐
│ Edit beads   │ ──────────────>│ .beads/issues.jsonl│ ──────────────────>│ GitHub Issues   │
│ issues.jsonl │                │ updated on main   │                    │ + Project board  │
└──────────────┘                └──────────────────┘                    └─────────────────┘
```

1. **Work locally** -- create and update beads tickets while developing on a feature branch.
2. **Commit together** -- beads issue changes are committed alongside code changes.
3. **Merge to main** -- when the PR merges, the GitHub Action triggers.
4. **Sync runs** -- the action reads `issues.jsonl`, creates or updates GitHub Issues, and moves cards on the Project board.

This is a **one-way sync** (beads → GitHub). The local `.beads/issues.jsonl` is the source of truth.

## File Layout

```
.beads/
  metadata.json       # Project prefix and config
  issues.jsonl        # All beads issues (one JSON object per line)
  github-map.json     # Mapping of beads ID → GitHub issue number (auto-generated)
  mq/                 # Merge queue (used by beads internally)

.github/
  scripts/
    beads-sync.sh     # Sync script (reads JSONL, calls gh CLI)
  workflows/
    beads-sync.yml    # GitHub Action that runs the sync on push to main
```

## Issue Format

Each line in `issues.jsonl` is a JSON object:

```json
{
  "id": "acs-0001",
  "title": "Electron installer for ACS distribution",
  "description": "Set up an Electron app that...",
  "status": "open",
  "priority": 1,
  "issue_type": "task",
  "created_at": "2026-02-09T00:00:00.000000-08:00",
  "updated_at": "2026-02-09T00:00:00.000000-08:00"
}
```

## Status Mapping

Beads statuses map to GitHub Project columns:

| Beads Status | Project Column |
|---|---|
| `open`, `pending` | Todo |
| `in_progress`, `hooked` | In Progress |
| `blocked` | Blocked |
| `closed`, `completed` | Done |

Issues with status `tombstone` or type `event` are skipped during sync.

## Labels

The sync script creates GitHub labels from beads fields:

- **Issue type** → `bug`, `enhancement`, `task`, `epic`
- **Priority** → `priority:critical` (0), `priority:high` (1), `priority:medium` (2), `priority:low` (3)

## Setup

### GitHub Issues Only (no Project board)

Works out of the box. The GitHub Action uses the built-in `GITHUB_TOKEN` which has `issues: write` permission. No additional configuration needed.

### With GitHub Projects Board

1. Create a GitHub Project at `github.com/users/<owner>/projects`.
2. Add a **Status** field (single select) with options: `Todo`, `In Progress`, `Blocked`, `Done`.
3. Set `PROJECT_NUMBER` in `.github/workflows/beads-sync.yml` to the project number from the URL.
4. The `GITHUB_TOKEN` needs the `project` scope. For organization projects, you may need a PAT with `project` permissions stored as a repository secret.

### Manual / Local Runs

```sh
export GITHUB_REPOSITORY="Jtonna/agent-cron-scheduler"
export GH_TOKEN="$(gh auth token)"

# Dry run (preview only)
.github/scripts/beads-sync.sh --dry-run

# Actual sync
.github/scripts/beads-sync.sh
```

## ID Mapping

The sync script maintains `.beads/github-map.json` to track which beads issue corresponds to which GitHub issue number:

```json
{
  "acs-0001": 42,
  "acs-0002": 43
}
```

This prevents duplicate GitHub issues on re-runs. The map file is generated in CI and is not committed back (it's ephemeral per sync run). If you need persistent mapping, you can commit this file.

## Triggering

The action triggers on:

- **Push to `main`** when `.beads/issues.jsonl` is modified.
- **Manual dispatch** via the Actions tab (useful for initial sync or debugging).
