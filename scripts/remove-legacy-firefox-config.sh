#!/usr/bin/env bash
set -euo pipefail

echo "sensiblefox: remove legacy Firefox app-bundle config"
echo "====================================================="

app_roots=(
  "/Applications/Firefox.app"
  "$HOME/Applications/Firefox.app"
)

declare -a targets=()

for app_root in "${app_roots[@]}"; do
  resources="$app_root/Contents/Resources"
  defaults_pref="$resources/defaults/pref"
  distribution="$resources/distribution"

  targets+=(
    "$resources/mozilla.cfg"
    "$resources/sensiblefox.cfg"
    "$resources/sensiblefox"
    "$distribution/policies.json"
    "$defaults_pref/autoconfig.js"
    "$defaults_pref/local-settings.js"
    "$defaults_pref/sensiblefox.js"
    "$defaults_pref/sensiblefox-autoconfig.js"
    "$defaults_pref/firebuilder.js"
  )
done

removed=0
missing=0
failed=0

remove_path() {
  local path="$1"
  if [ ! -e "$path" ]; then
    echo "  - not found: $path"
    missing=$((missing + 1))
    return
  fi

  if [ -w "$path" ] || [ -w "$(dirname "$path")" ]; then
    rm -rf "$path"
  else
    sudo rm -rf "$path"
  fi

  if [ -e "$path" ]; then
    echo "  ✗ failed: $path"
    failed=$((failed + 1))
  else
    echo "  ✓ removed: $path"
    removed=$((removed + 1))
  fi
}

for path in "${targets[@]}"; do
  remove_path "$path"
done

echo ""
echo "Summary: removed=$removed missing=$missing failed=$failed"

if [ "$failed" -ne 0 ]; then
  exit 1
fi
