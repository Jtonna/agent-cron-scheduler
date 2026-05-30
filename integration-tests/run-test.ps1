#!/usr/bin/env pwsh
# Thin wrapper around _runner.py. Forwards all args.
# Usage:
#   .\integration-tests\run-test.ps1 [--daemon-url <url>] [--workflow <path>]
#                                    [--keep] [--input '<json>'] [--timeout-secs <n>]

$ErrorActionPreference = "Stop"

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$runner = Join-Path $here "_runner.py"

# Find a python interpreter.
$python = $null
foreach ($candidate in @("python", "python3", "py")) {
    $cmd = Get-Command $candidate -ErrorAction SilentlyContinue
    if ($cmd) { $python = $cmd.Source; break }
}
if (-not $python) {
    Write-Error "Could not find a Python 3 interpreter on PATH (tried: python, python3, py)."
    exit 2
}

& $python -B $runner @args
exit $LASTEXITCODE
