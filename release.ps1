$ErrorActionPreference = "Stop"

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
    "1" { $newVersion = $patchVersion }
    "2" { $newVersion = $minorVersion }
    "3" { $newVersion = $majorVersion }
    default {
        Write-Host "Invalid choice" -ForegroundColor Red
        exit 1
    }
}

Write-Host "Bumping to $newVersion" -ForegroundColor Green

# Update all package.json files
$packageFiles = @(
    "electron/package.json",
    "electron/packages/app/package.json"
)

foreach ($file in $packageFiles) {
    $content = Get-Content $file -Raw
    $json = $content | ConvertFrom-Json
    $json.version = $newVersion
    $json | ConvertTo-Json -Depth 10 | Set-Content $file -NoNewline
    Write-Host "Updated $file" -ForegroundColor Gray
}

# Update acs/Cargo.toml version (only first match = [package] version, anchored to start of line)
$cargoPath = "acs/Cargo.toml"
$cargoContent = Get-Content $cargoPath -Raw
$cargoContent = $cargoContent -replace '(?m)^version = ".*"', "version = `"$newVersion`""
Set-Content $cargoPath -Value $cargoContent -NoNewline
Write-Host "Updated $cargoPath" -ForegroundColor Gray

# Update acs/web/openapi.yaml version (info.version and example version — 2 occurrences)
$openapiPath = "acs/web/openapi.yaml"
$openapiContent = Get-Content $openapiPath -Raw
$openapiContent = $openapiContent -replace [regex]::Escape("version: `"$currentVersion`""), "version: `"$newVersion`""
Set-Content $openapiPath -Value $openapiContent -NoNewline
Write-Host "Updated $openapiPath" -ForegroundColor Gray

# Update lock file
Write-Host "Updating package-lock.json..."
Push-Location "electron"
npm install --package-lock-only --silent
Pop-Location

# Commit and tag
git add acs/Cargo.toml acs/web/openapi.yaml
git add -A
git commit -m "chore: bump version to $newVersion"
git tag "v$newVersion"

Write-Host ""
Write-Host "Version bumped to $newVersion" -ForegroundColor Green
Write-Host "Tag v$newVersion created" -ForegroundColor Green
Write-Host ""

$push = Read-Host "Push to origin? (y/n)"
if ($push -eq "y") {
    git push origin main
    git push origin "v$newVersion"
    Write-Host "Pushed! GitHub Actions will build the release." -ForegroundColor Green
} else {
    Write-Host "Run 'git push origin main && git push origin v$newVersion' when ready." -ForegroundColor Yellow
}
