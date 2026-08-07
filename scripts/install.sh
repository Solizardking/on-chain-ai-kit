#!/usr/bin/env bash
# One-shot install for OpenClawd Solana Kit
#
#   curl -fsSL https://raw.githubusercontent.com/clawdsolana/OpenClawd/main/scripts/install.sh | bash
#
# Or from a clone:
#   sh scripts/install.sh
#
# Installs: Node CLI (local npm link) + builds HTTP binary (cargo --features full)
# Docs: docs/installation.md · docs/configuration.md · docs/quickstart.md

set -euo pipefail

REPO_URL="${CLAWD_REPO_URL:-https://github.com/clawdsolana/OpenClawd.git}"
INSTALL_DIR="${CLAWD_INSTALL_DIR:-$HOME/.openclawd-solana-kit}"
BRANCH="${CLAWD_BRANCH:-main}"

echo "openclawd-kit: install starting"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "openclawd-kit: missing dependency: $1" >&2
    return 1
  fi
}

if ! command -v cargo >/dev/null 2>&1; then
  echo "openclawd-kit: cargo not found — installing rustup (non-interactive)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
fi

need cargo
need node
need npm
need git

# Prefer running inside an existing clone if this script lives in one
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_CANDIDATE="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"

if [ -f "$ROOT_CANDIDATE/Cargo.toml" ] && [ -f "$ROOT_CANDIDATE/package.json" ]; then
  ROOT="$ROOT_CANDIDATE"
  echo "openclawd-kit: using local checkout $ROOT"
else
  ROOT="$INSTALL_DIR"
  if [ -d "$ROOT/.git" ]; then
    echo "openclawd-kit: updating $ROOT"
    git -C "$ROOT" fetch --depth 1 origin "$BRANCH"
    git -C "$ROOT" checkout "$BRANCH"
    git -C "$ROOT" pull --ff-only origin "$BRANCH" || true
  else
    echo "openclawd-kit: cloning $REPO_URL → $ROOT"
    rm -rf "$ROOT"
    git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$ROOT"
  fi
fi

cd "$ROOT"

if [ ! -f .env ] && [ ! -f src/.env.local ]; then
  if [ -f .env.example ]; then
    cp .env.example .env
    echo "openclawd-kit: wrote .env from .env.example — fill PRIVY_* before start"
  fi
fi

npm install --no-fund --no-audit
npm run build:kit

# Local bin shim
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/openclawd-kit" <<EOF
#!/usr/bin/env bash
exec node "$ROOT/npm/bin/openclawd-kit.mjs" "\$@"
EOF
chmod +x "$HOME/.local/bin/openclawd-kit"

echo ""
echo "openclawd-kit: install complete"
echo "  root:   $ROOT"
echo "  cli:    $HOME/.local/bin/openclawd-kit   (ensure ~/.local/bin is on PATH)"
echo ""
echo "Next:"
echo "  openclawd-kit setup     # if you still need .env"
echo "  openclawd-kit doctor    # check PRIVY_* + Rust"
echo "  openclawd-kit start     # HTTP SSE on :6969"
echo ""
echo "Docs: $ROOT/docs/quickstart.md · configuration.md · http_service.md"
