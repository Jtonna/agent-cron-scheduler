#!/usr/bin/env bash
set -euo pipefail

# Check gh CLI
if ! command -v gh &> /dev/null; then
    echo "Error: GitHub CLI (gh) is not installed. Install from https://cli.github.com/"
    exit 1
fi

# Check branch
BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$BRANCH" != "main" ]; then
    echo "Error: Must be on main branch (currently on $BRANCH)"
    exit 1
fi

# Check for uncommitted changes
if [ -n "$(git status --porcelain)" ]; then
    echo "Error: Uncommitted changes detected. Commit or stash first."
    exit 1
fi

# Read current version
CURRENT_VERSION=$(node -e "console.log(require('./electron/package.json').version)")
echo "Current version: $CURRENT_VERSION"

# Parse version parts
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

PATCH_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
MINOR_VERSION="$MAJOR.$((MINOR + 1)).0"
MAJOR_VERSION="$((MAJOR + 1)).0.0"

echo ""
echo "1) Patch: $PATCH_VERSION"
echo "2) Minor: $MINOR_VERSION"
echo "3) Major: $MAJOR_VERSION"
echo ""
read -rp "Select version bump (1/2/3): " CHOICE

case $CHOICE in
    1) BUMP_TYPE="patch" ;;
    2) BUMP_TYPE="minor" ;;
    3) BUMP_TYPE="major" ;;
    *) echo "Invalid choice"; exit 1 ;;
esac

echo "Triggering release workflow with $BUMP_TYPE bump..."
gh workflow run release.yml -f bump="$BUMP_TYPE"

REPO_URL=$(gh repo view --json url -q '.url')
echo ""
echo "Release workflow triggered! Monitor progress at:"
echo "$REPO_URL/actions/workflows/release.yml"
