#!/usr/bin/env bash
#
# comment-sync.sh — Post queued comments to GitHub Issues, then clear the queue.
#
# Reads .github/comment-queue.json (an array of {issue, body} objects),
# posts each as a comment via `gh issue comment`, then resets the file
# to [] and commits with [skip ci] to prevent recursive triggers.
#
# Required env vars:
#   GH_TOKEN — GitHub token (set automatically in GitHub Actions)

set -euo pipefail

QUEUE_FILE=".github/comment-queue.json"

# Exit early if queue file doesn't exist
if [[ ! -f "$QUEUE_FILE" ]]; then
  echo "No queue file found at $QUEUE_FILE"
  exit 0
fi

# Read queue length
QUEUE_LENGTH=$(jq 'length' "$QUEUE_FILE")

if [[ "$QUEUE_LENGTH" -eq 0 ]]; then
  echo "Queue is empty, nothing to do."
  exit 0
fi

echo "Processing $QUEUE_LENGTH queued comment(s)..."

POSTED=0
FAILED=0

for ((i = 0; i < QUEUE_LENGTH; i++)); do
  ISSUE=$(jq -r ".[$i].issue" "$QUEUE_FILE")
  BODY=$(jq -r ".[$i].body" "$QUEUE_FILE")

  if [[ -z "$ISSUE" || "$ISSUE" == "null" || -z "$BODY" || "$BODY" == "null" ]]; then
    echo "  [skip] Entry $i: missing issue or body"
    continue
  fi

  echo "  Posting comment to issue #${ISSUE}..."
  if gh issue comment "$ISSUE" --body "$BODY" 2>/dev/null; then
    echo "  [ok] Posted to #${ISSUE}"
    POSTED=$((POSTED + 1))
  else
    echo "  [fail] Could not post to #${ISSUE}"
    FAILED=$((FAILED + 1))
  fi
done

echo ""
echo "Posted: $POSTED, Failed: $FAILED"

# Clear the queue
echo "[]" > "$QUEUE_FILE"

# Commit and push the cleared queue
git config user.name "github-actions[bot]"
git config user.email "github-actions[bot]@users.noreply.github.com"
git add "$QUEUE_FILE"

if git diff --cached --quiet 2>/dev/null; then
  echo "No changes to commit (queue was already empty)."
else
  git commit -m "chore: clear comment queue [skip ci]"
  git push
  echo "Queue cleared and pushed."
fi
