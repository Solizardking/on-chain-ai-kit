#!/bin/sh
# Crustacean Automation — thin alias for crustacean-automation.sh
# curl -fsSL https://github.com/Solizardking/on-chain-ai-kit/raw/main/automaton/scripts/automaton.sh | sh
#
# Delegates to the Crustacean Automation / Clawd automaton installer.
set -e
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" 2>/dev/null && pwd)"
if [ -f "$SCRIPT_DIR/crustacean-automation.sh" ]; then
  exec sh "$SCRIPT_DIR/crustacean-automation.sh" "$@"
fi
# When piped via curl|sh, dirname may not resolve; fetch the primary installer
exec sh -c 'curl -fsSL https://github.com/Solizardking/on-chain-ai-kit/raw/main/automaton/scripts/crustacean-automation.sh | sh'
