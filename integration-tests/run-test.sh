#!/usr/bin/env bash
# Thin wrapper around _runner.py. Forwards all args.
# Usage:
#   ./integration-tests/run-test.sh [--daemon-url <url>] [--workflow <path>]
#                                   [--keep] [--input '<json>'] [--timeout-secs <n>]

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
runner="$here/_runner.py"

if command -v python3 >/dev/null 2>&1; then
    python="python3"
elif command -v python >/dev/null 2>&1; then
    python="python"
else
    echo "Could not find a Python 3 interpreter on PATH (tried: python3, python)." >&2
    exit 2
fi

exec "$python" -B "$runner" "$@"
