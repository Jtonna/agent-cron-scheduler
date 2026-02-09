#!/usr/bin/env bash
#
# beads-sync.sh — One-way sync from .beads/issues.jsonl to GitHub Issues + Project
#
# Reads beads issues, creates/updates GitHub Issues, and manages Project board columns.
# Tracks mapping between beads IDs and GitHub issue numbers in .beads/github-map.json.
#
# Required env vars:
#   GH_TOKEN          — GitHub token with repo + project scopes
#   GITHUB_REPOSITORY — owner/repo (set automatically in GitHub Actions)
#   PROJECT_NUMBER    — GitHub Project number to sync to
#
# Usage:
#   ./beads-sync.sh                    # sync all issues
#   ./beads-sync.sh --dry-run          # preview without making changes

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
BEADS_DIR="$REPO_ROOT/.beads"
ISSUES_JSONL="$BEADS_DIR/issues.jsonl"
GITHUB_MAP="$BEADS_DIR/github-map.json"
DRY_RUN=false

# Beads status -> GitHub Project column mapping
declare -A STATUS_MAP=(
  ["open"]="Todo"
  ["pending"]="Todo"
  ["in_progress"]="In Progress"
  ["blocked"]="Blocked"
  ["hooked"]="In Progress"
  ["closed"]="Done"
  ["completed"]="Done"
)

# Parse args
for arg in "$@"; do
  case $arg in
    --dry-run) DRY_RUN=true ;;
  esac
done

# Validate environment
if [[ -z "${GITHUB_REPOSITORY:-}" ]]; then
  echo "ERROR: GITHUB_REPOSITORY not set (expected owner/repo)"
  exit 1
fi

if [[ -z "${PROJECT_NUMBER:-}" ]]; then
  echo "WARNING: PROJECT_NUMBER not set — will sync GitHub Issues only (no Project board)"
fi

if [[ ! -f "$ISSUES_JSONL" ]]; then
  echo "No beads issues found at $ISSUES_JSONL"
  exit 0
fi

OWNER="${GITHUB_REPOSITORY%%/*}"
REPO="${GITHUB_REPOSITORY##*/}"

# Initialize github-map.json if it doesn't exist
if [[ ! -f "$GITHUB_MAP" ]]; then
  echo "{}" > "$GITHUB_MAP"
fi

# ── Helper functions ─────────────────────────────────────────────────────

get_gh_issue_number() {
  local beads_id="$1"
  jq -r --arg id "$beads_id" '.[$id] // empty' "$GITHUB_MAP"
}

set_gh_issue_number() {
  local beads_id="$1"
  local gh_number="$2"
  local tmp
  tmp=$(mktemp)
  jq --arg id "$beads_id" --arg num "$gh_number" '.[$id] = ($num | tonumber)' "$GITHUB_MAP" > "$tmp"
  mv "$tmp" "$GITHUB_MAP"
}

map_status_to_column() {
  local status="$1"
  echo "${STATUS_MAP[$status]:-Todo}"
}

# Build labels from beads issue_type
build_labels() {
  local issue_type="$1"
  local priority="$2"
  local labels=""

  case "$issue_type" in
    bug)     labels="bug" ;;
    feature) labels="enhancement" ;;
    task)    labels="task" ;;
    epic)    labels="epic" ;;
    *)       labels="task" ;;
  esac

  case "$priority" in
    0) labels="$labels,priority:critical" ;;
    1) labels="$labels,priority:high" ;;
    2) labels="$labels,priority:medium" ;;
    3) labels="$labels,priority:low" ;;
  esac

  echo "$labels"
}

# ── Project board helpers ────────────────────────────────────────────────

PROJECT_ID=""
STATUS_FIELD_ID=""
declare -A OPTION_IDS=()

setup_project_fields() {
  if [[ -z "${PROJECT_NUMBER:-}" ]]; then
    return
  fi

  echo "Fetching project metadata..."

  # Get project ID
  PROJECT_ID=$(gh project view "$PROJECT_NUMBER" --owner "$OWNER" --format json --jq '.id' 2>/dev/null || echo "")
  if [[ -z "$PROJECT_ID" ]]; then
    echo "WARNING: Could not find project #$PROJECT_NUMBER — skipping board sync"
    PROJECT_NUMBER=""
    return
  fi

  # Get the Status field ID and option IDs
  local fields_json
  fields_json=$(gh project field-list "$PROJECT_NUMBER" --owner "$OWNER" --format json 2>/dev/null || echo '{"fields":[]}')

  STATUS_FIELD_ID=$(echo "$fields_json" | jq -r '.fields[] | select(.name == "Status") | .id' 2>/dev/null || echo "")

  if [[ -n "$STATUS_FIELD_ID" ]]; then
    # Extract option IDs for each status value
    while IFS=$'\t' read -r opt_name opt_id; do
      OPTION_IDS["$opt_name"]="$opt_id"
    done < <(echo "$fields_json" | jq -r '.fields[] | select(.name == "Status") | .options[]? | [.name, .id] | @tsv' 2>/dev/null || true)
  fi

  echo "Project ID: $PROJECT_ID"
  echo "Status field: $STATUS_FIELD_ID"
  echo "Status options: ${!OPTION_IDS[*]}"
}

update_project_item_status() {
  local gh_issue_number="$1"
  local column="$2"

  if [[ -z "${PROJECT_NUMBER:-}" || -z "$STATUS_FIELD_ID" ]]; then
    return
  fi

  local option_id="${OPTION_IDS[$column]:-}"
  if [[ -z "$option_id" ]]; then
    echo "  WARNING: No project option for column '$column' — skipping board update"
    return
  fi

  # Find the project item ID for this issue
  local issue_url="https://github.com/$GITHUB_REPOSITORY/issues/$gh_issue_number"
  local item_id
  item_id=$(gh project item-list "$PROJECT_NUMBER" --owner "$OWNER" --format json --limit 200 \
    | jq -r --arg url "$issue_url" '.items[] | select(.content.url == $url) | .id' 2>/dev/null || echo "")

  if [[ -z "$item_id" ]]; then
    # Item not in project yet — add it
    echo "  Adding to project #$PROJECT_NUMBER..."
    if [[ "$DRY_RUN" == "true" ]]; then
      echo "  [DRY RUN] Would add issue #$gh_issue_number to project"
    else
      item_id=$(gh project item-add "$PROJECT_NUMBER" --owner "$OWNER" --url "$issue_url" --format json | jq -r '.id' 2>/dev/null || echo "")
    fi
  fi

  if [[ -n "$item_id" && "$DRY_RUN" == "false" ]]; then
    echo "  Setting status to '$column'..."
    gh project item-edit --id "$item_id" --project-id "$PROJECT_ID" \
      --field-id "$STATUS_FIELD_ID" --single-select-option-id "$option_id" 2>/dev/null || \
      echo "  WARNING: Failed to update project item status"
  elif [[ "$DRY_RUN" == "true" ]]; then
    echo "  [DRY RUN] Would set status to '$column'"
  fi
}

# ── Main sync loop ───────────────────────────────────────────────────────

echo "=== Beads → GitHub Sync ==="
echo "Repository: $GITHUB_REPOSITORY"
echo "Dry run: $DRY_RUN"
echo ""

# Setup project board if configured
setup_project_fields

# Ensure labels exist
if [[ "$DRY_RUN" == "false" ]]; then
  for label in task epic "priority:critical" "priority:high" "priority:medium" "priority:low"; do
    gh label create "$label" --repo "$GITHUB_REPOSITORY" --force 2>/dev/null || true
  done
fi

CREATED=0
UPDATED=0
SKIPPED=0

while IFS= read -r line; do
  # Skip empty lines
  [[ -z "$line" ]] && continue

  # Parse issue fields
  beads_id=$(echo "$line" | jq -r '.id')
  title=$(echo "$line" | jq -r '.title')
  description=$(echo "$line" | jq -r '.description // ""')
  status=$(echo "$line" | jq -r '.status // "open"')
  issue_type=$(echo "$line" | jq -r '.issue_type // "task"')
  priority=$(echo "$line" | jq -r '.priority // 2')

  # Skip tombstones and ephemeral events
  if [[ "$status" == "tombstone" ]] || [[ "$issue_type" == "event" ]]; then
    SKIPPED=$((SKIPPED + 1))
    continue
  fi

  column=$(map_status_to_column "$status")
  labels=$(build_labels "$issue_type" "$priority")

  # Build body with beads metadata footer
  body="${description}

---
*Synced from beads \`$beads_id\` | type: $issue_type | priority: $priority*"

  # Check if we already have a GitHub issue for this beads ID
  gh_number=$(get_gh_issue_number "$beads_id")

  if [[ -n "$gh_number" ]]; then
    # Update existing issue
    echo "Updating: $beads_id → GitHub #$gh_number ($title)"

    if [[ "$DRY_RUN" == "true" ]]; then
      echo "  [DRY RUN] Would update issue #$gh_number"
    else
      # Update title, body, labels
      gh issue edit "$gh_number" \
        --repo "$GITHUB_REPOSITORY" \
        --title "[$beads_id] $title" \
        --body "$body" \
        --add-label "$labels" 2>/dev/null || echo "  WARNING: Failed to update issue #$gh_number"

      # Close/reopen based on status
      if [[ "$status" == "closed" || "$status" == "completed" ]]; then
        gh issue close "$gh_number" --repo "$GITHUB_REPOSITORY" 2>/dev/null || true
      else
        gh issue reopen "$gh_number" --repo "$GITHUB_REPOSITORY" 2>/dev/null || true
      fi
    fi

    update_project_item_status "$gh_number" "$column"
    UPDATED=$((UPDATED + 1))
  else
    # Create new issue
    echo "Creating: $beads_id → [$beads_id] $title"

    if [[ "$DRY_RUN" == "true" ]]; then
      echo "  [DRY RUN] Would create new GitHub issue"
      CREATED=$((CREATED + 1))
    else
      # Create the issue
      new_url=$(gh issue create \
        --repo "$GITHUB_REPOSITORY" \
        --title "[$beads_id] $title" \
        --body "$body" \
        --label "$labels" 2>/dev/null || echo "")

      if [[ -n "$new_url" ]]; then
        new_number=$(echo "$new_url" | grep -oP '\d+$')
        set_gh_issue_number "$beads_id" "$new_number"
        echo "  Created GitHub issue #$new_number"

        # Close if beads status is closed
        if [[ "$status" == "closed" || "$status" == "completed" ]]; then
          gh issue close "$new_number" --repo "$GITHUB_REPOSITORY" 2>/dev/null || true
        fi

        update_project_item_status "$new_number" "$column"
        CREATED=$((CREATED + 1))
      else
        echo "  ERROR: Failed to create GitHub issue for $beads_id"
      fi
    fi
  fi

done < "$ISSUES_JSONL"

echo ""
echo "=== Sync Complete ==="
echo "Created: $CREATED"
echo "Updated: $UPDATED"
echo "Skipped: $SKIPPED"
