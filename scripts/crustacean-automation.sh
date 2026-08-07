#!/bin/sh
# Root convenience wrapper — delegates to automaton/scripts/crustacean-automation.sh
# Usage from kit root:
#   CLAWD_SKIP_START=1 sh scripts/crustacean-automation.sh
set -e
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export CLAWD_KIT_ROOT="${CLAWD_KIT_ROOT:-$ROOT}"
export CLAWD_LOCAL="${CLAWD_LOCAL:-1}"
exec sh "$ROOT/automaton/scripts/crustacean-automation.sh" "$@"
