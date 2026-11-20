#!/usr/bin/env bash
set -euo pipefail

: "${CHOCOLATEY_API_KEY:?Set CHOCOLATEY_API_KEY before publishing}"

shopt -s nullglob
for package_id in zar-rs zar-rs-v3; do
    packages=("$package_id".*.nupkg)
    if [[ ${#packages[@]} -ne 1 ]]; then
        echo "Expected exactly one $package_id package in the current directory" >&2
        exit 1
    fi
    choco push "${packages[0]}" --source https://push.chocolatey.org/ \
        -k "$CHOCOLATEY_API_KEY"
done
