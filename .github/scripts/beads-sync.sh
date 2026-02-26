#!/usr/bin/env bash
#
# beads-sync.sh — One-way sync from beads issues to GitHub Issues + Project
#
# Supports both storage formats:
#   - .beads/issues.jsonl   (legacy single-file format)
#   - .beads/issues/        (newer directory-based format, one JSON file per issue)
#
# Syncs: title, description, status, labels, project board, comments, close_reason
# Tracks mappings in .beads/github-map.json and .beads/comment-map.json
#
# Required env vars:
#   GH_TOKEN          — GitHub token with repo + project scopes
#   GITHUB_REPOSITORY — owner/repo (set automatically in GitHub Actions)
#   PROJECT_NUMBER    — GitHub Project number to sync to (optional)
#
# Usage:
#   ./beads-sync.sh                    # sync all issues
#   ./beads-sync.sh --dry-run          # preview without making changes

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
BEADS_DIR="$REPO_ROOT/.beads"
ISSUES_JSONL="$BEADS_DIR/issues.jsonl"
ISSUES_DIR="$BEADS_DIR/issues"
GITHUB_MAP="$BEADS_DIR/github-map.json"
COMMENT_MAP="$BEADS_DIR/comment-map.json"
DRY_RUN=false

# Branch awareness — defer issue closure on non-default branches
BEADS_BRANCH="${BEADS_BRANCH:-main}"
BEADS_DEFAULT_BRANCH="${BEADS_DEFAULT_BRANCH:-main}"
IS_DEFAULT_BRANCH=false
if [[ "$BEADS_BRANCH" == "$BEADS_DEFAULT_BRANCH" ]]; then
  IS_DEFAULT_BRANCH=true
fi

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

OWNER="${GITHUB_REPOSITORY%%/*}"
REPO="${GITHUB_REPOSITORY##*/}"

# Initialize map files if they don't exist
if [[ ! -f "$GITHUB_MAP" ]]; then
  echo "{}" > "$GITHUB_MAP"
fi

if [[ ! -f "$COMMENT_MAP" ]]; then
  echo "{}" > "$COMMENT_MAP"
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

# Get list of already-synced comment keys for a beads issue
# Key format: "beads_id:comment_id" or "beads_id:close_reason"
is_comment_synced() {
  local key="$1"
  local result
  result=$(jq -r --arg k "$key" '.[$k] // empty' "$COMMENT_MAP")
  [[ -n "$result" ]]
}

mark_comment_synced() {
  local key="$1"
  local gh_comment_url="$2"
  local tmp
  tmp=$(mktemp)
  jq --arg k "$key" --arg v "$gh_comment_url" '.[$k] = $v' "$COMMENT_MAP" > "$tmp"
  mv "$tmp" "$COMMENT_MAP"
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

# Handle close/reopen with branch awareness
handle_close_reopen() {
  local gh_number="$1"
  local status="$2"
  local beads_id="$3"

  if [[ "$status" == "closed" || "$status" == "completed" ]]; then
    if [[ "$IS_DEFAULT_BRANCH" == "true" ]]; then
      # On default branch: actually close and clean up pending-close label
      gh issue close "$gh_number" --repo "$GITHUB_REPOSITORY" 2>/dev/null || true
      gh issue edit "$gh_number" --repo "$GITHUB_REPOSITORY" --remove-label "pending-close" 2>/dev/null || true
    else
      # On feature branch: mark as pending-close, don't actually close
      gh issue edit "$gh_number" --repo "$GITHUB_REPOSITORY" --add-label "pending-close" 2>/dev/null || true
      local pc_key="${beads_id}:pending_close:${BEADS_BRANCH}"
      if ! is_comment_synced "$pc_key"; then
        local pc_body="Marked as closed on branch \`${BEADS_BRANCH}\` — will close when merged to \`${BEADS_DEFAULT_BRANCH}\`.

---
*Synced from beads \`${beads_id}\`*"
        local pc_url
        pc_url=$(gh issue comment "$gh_number" \
          --repo "$GITHUB_REPOSITORY" \
          --body "$pc_body" 2>/dev/null || echo "")
        if [[ -n "$pc_url" ]]; then
          mark_comment_synced "$pc_key" "$pc_url"
        fi
      fi
    fi
  else
    # Not closed — reopen if needed and clean up pending-close
    gh issue reopen "$gh_number" --repo "$GITHUB_REPOSITORY" 2>/dev/null || true
    gh issue edit "$gh_number" --repo "$GITHUB_REPOSITORY" --remove-label "pending-close" 2>/dev/null || true
  fi
}

# ── Comment sync ─────────────────────────────────────────────────────────

sync_comments() {
  local beads_id="$1"
  local gh_number="$2"
  local issue_json="$3"

  # Sync close_reason as a comment if present and not already synced
  local close_reason
  close_reason=$(echo "$issue_json" | jq -r '.close_reason // empty')
  if [[ -n "$close_reason" ]]; then
    local cr_key="${beads_id}:close_reason"
    if ! is_comment_synced "$cr_key"; then
      local cr_body="**Close reason:** ${close_reason}

---
*Synced from beads \`${beads_id}\`*"
      echo "  Posting close_reason as comment..."
      if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [DRY RUN] Would post close_reason comment"
      else
        local cr_url
        cr_url=$(gh issue comment "$gh_number" \
          --repo "$GITHUB_REPOSITORY" \
          --body "$cr_body" 2>/dev/null || echo "")
        if [[ -n "$cr_url" ]]; then
          mark_comment_synced "$cr_key" "$cr_url"
          echo "  Posted close_reason comment"
        else
          echo "  WARNING: Failed to post close_reason comment"
        fi
      fi
    fi
  fi

  # Sync each comment in the comments array
  local comment_count
  comment_count=$(echo "$issue_json" | jq '.comments // [] | length')

  if [[ "$comment_count" -eq 0 ]]; then
    return
  fi

  local i
  for ((i = 0; i < comment_count; i++)); do
    local comment_id
    comment_id=$(echo "$issue_json" | jq -r ".comments[$i].id")
    local comment_key="${beads_id}:comment:${comment_id}"

    if is_comment_synced "$comment_key"; then
      continue
    fi

    local author
    author=$(echo "$issue_json" | jq -r ".comments[$i].author // \"unknown\"")
    local text
    text=$(echo "$issue_json" | jq -r ".comments[$i].text // \"\"")
    local created_at
    created_at=$(echo "$issue_json" | jq -r ".comments[$i].created_at // \"\"")

    if [[ -z "$text" ]]; then
      continue
    fi

    local comment_body="**${author}** commented"
    if [[ -n "$created_at" ]]; then
      comment_body="${comment_body} on ${created_at}"
    fi
    comment_body="${comment_body}:

${text}

---
*Synced from beads \`${beads_id}\` comment #${comment_id}*"

    echo "  Posting comment #${comment_id}..."
    if [[ "$DRY_RUN" == "true" ]]; then
      echo "  [DRY RUN] Would post comment #${comment_id}"
    else
      local c_url
      c_url=$(gh issue comment "$gh_number" \
        --repo "$GITHUB_REPOSITORY" \
        --body "$comment_body" 2>/dev/null || echo "")
      if [[ -n "$c_url" ]]; then
        mark_comment_synced "$comment_key" "$c_url"
        echo "  Posted comment #${comment_id}"
      else
        echo "  WARNING: Failed to post comment #${comment_id}"
      fi
    fi
  done
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

# ── Issue reader (supports both formats) ─────────────────────────────────

# Collects all issues as JSON lines into a temp file, regardless of storage format
collect_issues() {
  local outfile="$1"

  if [[ -d "$ISSUES_DIR" ]] && ls "$ISSUES_DIR"/*.json >/dev/null 2>&1; then
    # Preferred: one JSON file per issue under .beads/issues/
    echo "Reading issues from .beads/issues/ directory..."
    > "$outfile"
    for issue_file in "$ISSUES_DIR"/*.json; do
      [[ -f "$issue_file" ]] || continue
      # Each file is a single JSON object — output as one line
      jq -c '.' "$issue_file" >> "$outfile"
    done
  elif [[ -f "$ISSUES_JSONL" ]] && [[ -s "$ISSUES_JSONL" ]]; then
    # Fallback: legacy single JSONL file (only if non-empty)
    echo "Reading issues from issues.jsonl..."
    cp "$ISSUES_JSONL" "$outfile"
  else
    echo "No beads issues found (checked .beads/issues/ and issues.jsonl)"
    exit 0
  fi
}

# ── Main sync loop ───────────────────────────────────────────────────────

echo "=== Beads → GitHub Sync ==="
echo "Repository: $GITHUB_REPOSITORY"
echo "Dry run: $DRY_RUN"
echo "Branch: $BEADS_BRANCH (default branch: $IS_DEFAULT_BRANCH)"
echo ""

# Setup project board if configured
setup_project_fields

# Ensure labels exist
if [[ "$DRY_RUN" == "false" ]]; then
  for label in task epic "priority:critical" "priority:high" "priority:medium" "priority:low" "pending-close"; do
    gh label create "$label" --repo "$GITHUB_REPOSITORY" --force 2>/dev/null || true
  done
fi

# Collect issues from whichever format is available
ISSUES_TMP=$(mktemp)
trap 'rm -f "$ISSUES_TMP"' EXIT
collect_issues "$ISSUES_TMP"

CREATED=0
UPDATED=0
SKIPPED=0
COMMENTS_SYNCED=0

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

      # Close/reopen based on status (branch-aware)
      handle_close_reopen "$gh_number" "$status" "$beads_id"
    fi

    # Sync comments
    sync_comments "$beads_id" "$gh_number" "$line"

    update_project_item_status "$gh_number" "$column"
    UPDATED=$((UPDATED + 1))
  else
    # Before creating, check if issue already exists by searching title (dedup safety net)
    existing=$(gh issue list --repo "$GITHUB_REPOSITORY" --search "[$beads_id]" --state all --json number --jq '.[0].number // empty' 2>/dev/null || true)
    if [[ -n "$existing" ]]; then
      echo "Found existing GitHub issue #$existing for $beads_id (map was stale), updating map"
      set_gh_issue_number "$beads_id" "$existing"
      gh_number="$existing"

      # Update the existing issue
      if [[ "$DRY_RUN" == "false" ]]; then
        gh issue edit "$gh_number" \
          --repo "$GITHUB_REPOSITORY" \
          --title "[$beads_id] $title" \
          --body "$body" \
          --add-label "$labels" 2>/dev/null || echo "  WARNING: Failed to update issue #$gh_number"

        handle_close_reopen "$gh_number" "$status" "$beads_id"
      fi

      # Sync comments
      sync_comments "$beads_id" "$gh_number" "$line"

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

          # Close/reopen based on status (branch-aware)
          handle_close_reopen "$new_number" "$status" "$beads_id"

          # Sync comments
          sync_comments "$beads_id" "$new_number" "$line"

          update_project_item_status "$new_number" "$column"
          CREATED=$((CREATED + 1))
        else
          echo "  ERROR: Failed to create GitHub issue for $beads_id"
        fi
      fi
    fi
  fi

done < "$ISSUES_TMP"

echo ""
echo "=== Sync Complete ==="
echo "Created: $CREATED"
echo "Updated: $UPDATED"
echo "Skipped: $SKIPPED"
