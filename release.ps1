$ErrorActionPreference = "Stop"

# Check gh CLI
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    Write-Host "Error: GitHub CLI (gh) is not installed. Install from https://cli.github.com/" -ForegroundColor Red
    exit 1
}

# Check branch
$branch = git rev-parse --abbrev-ref HEAD
if ($branch -ne "main") {
    Write-Host "Error: Must be on main branch (currently on $branch)" -ForegroundColor Red
    exit 1
}

# Check for uncommitted changes
$status = git status --porcelain
if ($status) {
    Write-Host "Error: Uncommitted changes detected. Commit or stash first." -ForegroundColor Red
    exit 1
}

# Read current version
$rootPkg = Get-Content "electron/package.json" | ConvertFrom-Json
$currentVersion = $rootPkg.version
Write-Host "Current version: $currentVersion" -ForegroundColor Cyan

# Parse version parts
$parts = $currentVersion.Split(".")
$major = [int]$parts[0]
$minor = [int]$parts[1]
$patch = [int]$parts[2]

# Show options
$patchVersion = "$major.$minor.$($patch + 1)"
$minorVersion = "$major.$($minor + 1).0"
$majorVersion = "$($major + 1).0.0"

Write-Host ""
Write-Host "1) Patch: $patchVersion"
Write-Host "2) Minor: $minorVersion"
Write-Host "3) Major: $majorVersion"
Write-Host ""
$choice = Read-Host "Select version bump (1/2/3)"

switch ($choice) {
    "1" { $bumpType = "patch" }
    "2" { $bumpType = "minor" }
    "3" { $bumpType = "major" }
    default {
        Write-Host "Invalid choice" -ForegroundColor Red
        exit 1
    }
}

Write-Host "Triggering release workflow with $bumpType bump..." -ForegroundColor Green
gh workflow run release.yml -f bump="$bumpType"

$repoUrl = gh repo view --json url -q '.url'
Write-Host ""
Write-Host "Release workflow triggered! Monitor progress at:" -ForegroundColor Green
Write-Host "$repoUrl/actions/workflows/release.yml" -ForegroundColor Cyan
