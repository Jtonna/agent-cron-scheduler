#!/usr/bin/env bash
set -euo pipefail

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
    1) NEW_VERSION=$PATCH_VERSION ;;
    2) NEW_VERSION=$MINOR_VERSION ;;
    3) NEW_VERSION=$MAJOR_VERSION ;;
    *) echo "Invalid choice"; exit 1 ;;
esac

echo "Bumping to $NEW_VERSION"

# Update all package.json files
for FILE in electron/package.json electron/packages/app/package.json; do
    node -e "
        const fs = require('fs');
        const pkg = JSON.parse(fs.readFileSync('$FILE', 'utf8'));
        pkg.version = '$NEW_VERSION';
        fs.writeFileSync('$FILE', JSON.stringify(pkg, null, 2) + '\n');
    "
    echo "Updated $FILE"
done

# Update lock file
echo "Updating package-lock.json..."
(cd electron && npm install --package-lock-only --silent)

# Commit and tag
git add -A
git commit -m "chore: bump version to $NEW_VERSION"
git tag "v$NEW_VERSION"

echo ""
echo "Version bumped to $NEW_VERSION"
echo "Tag v$NEW_VERSION created"
echo ""

read -rp "Push to origin? (y/n): " PUSH
if [ "$PUSH" = "y" ]; then
    git push origin main
    git push origin "v$NEW_VERSION"
    echo "Pushed! GitHub Actions will build the release."
else
    echo "Run 'git push origin main && git push origin v$NEW_VERSION' when ready."
fi
