#!/bin/sh
# On-Chain AI Kit - Automaton Installer
# curl -fsSL https://github.com/Solizardking/on-chain-ai-kit/raw/main/automaton/scripts/automaton.sh | sh
set -e
REPO="https://github.com/Solizardking/on-chain-ai-kit.git"
DIR="/opt/on-chain-automaton"
echo "==> Cloning On-Chain AI Kit (automaton)..."
git clone --depth 1 "$REPO" "$DIR"
cd "$DIR/automaton"
echo "==> Installing dependencies..."
npm install && npm run build
echo "==> Starting automaton..."
exec node dist/index.js --run