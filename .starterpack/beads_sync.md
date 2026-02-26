# Beads → GitHub Sync

One-way sync from beads issues to GitHub Issues (and optionally GitHub Projects boards).

## How It Works

A GitHub Actions workflow (`.github/workflows/beads-sync.yml`) triggers on every push to any branch that modifies `.beads/issues.jsonl` or `.beads/issues/**`. It runs a bash script (`.github/scripts/beads-sync.sh`) that is branch-aware: on feature branches it syncs everything but defers issue closure (adds `pending-close` label); on the default branch it performs full sync including actual closure.

1. Reads all beads issues (supports both JSONL and directory-based formats)
2. Creates or updates corresponding GitHub Issues
3. Syncs comments and close reasons
4. Applies labels based on `issue_type` and `priority`
5. Optionally updates a GitHub Projects board with status columns
6. Commits updated map files back to the repo (default branch only)

## Files

| File | Purpose |
|------|---------|
| `.github/workflows/beads-sync.yml` | GitHub Actions workflow trigger |
| `.github/scripts/beads-sync.sh` | Sync logic script |
| `.beads/github-map.json` | Maps beads IDs → GitHub issue numbers (auto-generated) |
| `.beads/comment-map.json` | Tracks which comments have been synced (auto-generated) |
| `.starterpack/hooks/post-merge` | Enhanced post-merge hook with auto-commit (source, deployed to .git/hooks/) |

## Trigger Conditions

- **Automatic:** Push to **any branch** that modifies `.beads/issues.jsonl` or `.beads/issues/**`
- **Manual:** `workflow_dispatch` from GitHub Actions UI (runs with default branch behavior)

## Branch-Aware Behavior

The sync script is branch-aware. Its behavior differs based on whether the push is to the default branch or a feature branch:

### Default branch (main)

Full sync — creates, updates, and **closes** GitHub Issues. Removes the `pending-close` label from any issues that were previously marked. Map files (`.beads/github-map.json`, `.beads/comment-map.json`) are committed back to the repo.

### Feature branches

Syncs everything **except actual issue closure**. When an issue has status `closed` or `completed` on a feature branch:

1. A `pending-close` label is added to the GitHub Issue
2. A comment is posted: "Marked as closed on branch `X` — will close when merged to `main`"
3. The GitHub Issue remains **open** until the branch merges to the default branch

Map files are **not** committed back on feature branches to avoid cross-branch divergence. The existing deduplication safety net (title search fallback) handles stale maps.

## Post-Merge Auto-Commit

The starterpack includes an enhanced post-merge git hook (`.starterpack/hooks/post-merge`) that is installed to `.git/hooks/post-merge` by the installer. This hook:

1. Imports updated beads issues after `git pull` or merge (standard beads behavior)
2. **Auto-commits** any `.beads/` file changes that result from the import

This prevents orphaned `.beads/issues.jsonl` changes that would otherwise accumulate after pulling merged PRs. The auto-commit uses the message `"chore: sync beads after merge [skip ci]"` and only stages `.beads/` files.

The `[skip ci]` tag prevents the GitHub Action from triggering on these housekeeping commits.

## Label Mapping

### Issue Type → GitHub Label

| Beads `issue_type` | GitHub Label |
|--------------------|-------------|
| `bug` | `bug` |
| `feature` | `enhancement` |
| `task` | `task` |
| `epic` | `epic` |
| *(other)* | `task` |

### Priority → GitHub Label

| Beads `priority` | GitHub Label |
|-------------------|-------------|
| `0` | `priority:critical` |
| `1` | `priority:high` |
| `2` | `priority:medium` |
| `3` | `priority:low` |

### Special Labels

| Label | Purpose |
|-------|---------|
| `pending-close` | Applied on feature branches when an issue is closed in beads but the branch hasn't merged yet |

## Status Mapping (for GitHub Projects)

| Beads Status | Project Column |
|-------------|---------------|
| `open` | Todo |
| `pending` | Todo |
| `in_progress` | In Progress |
| `blocked` | Blocked |
| `hooked` | In Progress |
| `closed` | Done |
| `completed` | Done |

## Configuration

### GitHub Issues Only (default)

No configuration needed. The workflow uses `GITHUB_TOKEN` which is automatically provided by GitHub Actions.

### With GitHub Projects Board

1. Create a GitHub Project at `github.com/users/<username>/projects`
2. Note the project number from the URL
3. Edit `.github/workflows/beads-sync.yml` and set `PROJECT_NUMBER`:
   ```yaml
   env:
     PROJECT_NUMBER: "5"  # your project number
   ```
4. Ensure the project has a "Status" field with options matching the status mapping above

## Deduplication

The sync script prevents duplicate GitHub Issues through two mechanisms:

1. **Map file:** `.beads/github-map.json` tracks which beads IDs have already been created as GitHub Issues
2. **Title search fallback:** If the map is stale (e.g. after a fresh clone), the script searches GitHub Issues by title pattern `[beads-id]` before creating a new one

## Running Locally

```bash
export GH_TOKEN="$(gh auth token)"
export GITHUB_REPOSITORY="owner/repo"
export PROJECT_NUMBER=""  # optional
export BEADS_BRANCH="main"           # or current branch name
export BEADS_DEFAULT_BRANCH="main"   # default branch
.github/scripts/beads-sync.sh --dry-run  # preview
.github/scripts/beads-sync.sh            # sync
```

## Permissions

The workflow requires these GitHub Actions permissions:
- `issues: write` — create/update/close GitHub Issues
- `contents: write` — commit map files back to repo
- `repository-projects: write` — update GitHub Projects board (if configured)
