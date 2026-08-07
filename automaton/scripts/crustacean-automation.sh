#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# Crustacean Automation — Clawd Automaton + OpenClawd Solana Kit Installer
# One-shot install for the Clawd / Crustacean Automation agent runtime.
#
# curl -fsSL https://github.com/Solizardking/on-chain-ai-kit/raw/main/automaton/scripts/crustacean-automation.sh | sh
#
# Local checkout (no re-clone):
#   CLAWD_LOCAL=1 sh automaton/scripts/crustacean-automation.sh
#   # or run from repo root / this script path when Cargo.toml + src/ exist
#
# Builds:
#   - TypeScript automaton (automaton/)
#   - Rust OpenClawd kit (src/ → target/debug|release) including solana, reasoning_loop,
#     signer, http (optional), bin/kit, dexscreener, wallet_manager, etc.
#
# Clones this repo's Clawd surface (not Conway-Research / conway.tech).
# The shell molts. The laws do not.
# ═══════════════════════════════════════════════════════════════════════════
set -e

REPO="${CLAWD_AUTOMATON_REPO:-https://github.com/Solizardking/on-chain-ai-kit.git}"
DIR="${CLAWD_AUTOMATON_DIR:-/opt/clawd-automaton}"
BRANCH="${CLAWD_AUTOMATON_BRANCH:-main}"
# automaton | kit | both  (default: both)
RUN_MODE="${CLAWD_RUN_MODE:-both}"
# cargo profile: debug (default) or release
CARGO_PROFILE="${CLAWD_CARGO_PROFILE:-debug}"
# default kit features: solana only; set CLAWD_KIT_FEATURES=full for http bin/kit
KIT_FEATURES="${CLAWD_KIT_FEATURES:-}"
SKIP_START="${CLAWD_SKIP_START:-0}"

# Resolve script location when not piped through curl|sh
SCRIPT_PATH="$0"
case "$SCRIPT_PATH" in
  /*) ;;
  *) SCRIPT_PATH="$(pwd)/$SCRIPT_PATH" ;;
esac
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$SCRIPT_PATH")" 2>/dev/null && pwd || true)"

# Prefer an existing local kit checkout (src/ + Cargo.toml + automaton/)
detect_local_root() {
  # 1) Explicit local mode or env root
  if [ -n "${CLAWD_KIT_ROOT:-}" ] && [ -f "${CLAWD_KIT_ROOT}/Cargo.toml" ] && [ -d "${CLAWD_KIT_ROOT}/src" ]; then
    echo "${CLAWD_KIT_ROOT}"
    return 0
  fi
  # 2) Script lives under <repo>/automaton/scripts/
  if [ -n "$SCRIPT_DIR" ] && [ -f "$SCRIPT_DIR/../../Cargo.toml" ] && [ -d "$SCRIPT_DIR/../../src" ]; then
    CDPATH= cd -- "$SCRIPT_DIR/../.." 2>/dev/null && pwd
    return 0
  fi
  # 3) Current working directory is the kit root
  if [ -f "./Cargo.toml" ] && [ -d "./src" ] && [ -d "./automaton" ]; then
    pwd
    return 0
  fi
  # 4) CLAWD_LOCAL=1 with DIR already a checkout
  if [ "${CLAWD_LOCAL:-0}" = "1" ] && [ -f "$DIR/Cargo.toml" ] && [ -d "$DIR/src" ]; then
    echo "$DIR"
    return 0
  fi
  return 1
}

echo ""
echo "  🦞  Crustacean Automation — Clawd Automaton + Kit Installer"
echo "  ──────────────────────────────────────────────────────────"
echo "  Mode:   $RUN_MODE"
echo "  Branch: $BRANCH"
echo ""

LOCAL_ROOT=""
if LOCAL_ROOT="$(detect_local_root)"; then
  DIR="$LOCAL_ROOT"
  echo "==> Using local kit checkout: $DIR"
else
  echo "  Repo:   $REPO"
  echo "  Target: $DIR"
  if [ -d "$DIR/.git" ]; then
    echo "==> Existing install found at $DIR — pulling latest..."
    git -C "$DIR" fetch --depth 1 origin "$BRANCH"
    git -C "$DIR" checkout "$BRANCH"
    git -C "$DIR" pull --ff-only origin "$BRANCH" || true
  else
    echo "==> Cloning Clawd on-chain-ai-kit..."
    git clone --depth 1 --branch "$BRANCH" "$REPO" "$DIR"
  fi
fi

# ── Validate kit surface ──────────────────────────────────────────────────
if [ ! -f "$DIR/Cargo.toml" ]; then
  echo "ERROR: Cargo.toml missing at $DIR (OpenClawd kit root required)" >&2
  exit 1
fi
if [ ! -d "$DIR/src" ] || [ ! -f "$DIR/src/lib.rs" ]; then
  echo "ERROR: kit src/ layout missing under $DIR/src" >&2
  exit 1
fi
for need in common.rs reasoning_loop.rs; do
  if [ ! -f "$DIR/src/$need" ]; then
    echo "ERROR: expected $DIR/src/$need" >&2
    exit 1
  fi
done
for need_dir in solana signer dexscreener data wallet_manager bin; do
  if [ ! -d "$DIR/src/$need_dir" ] && [ ! -f "$DIR/src/$need_dir" ]; then
    echo "WARN: optional/missing kit path src/$need_dir"
  fi
done
if [ ! -d "$DIR/automaton" ] || [ ! -f "$DIR/automaton/package.json" ]; then
  echo "ERROR: automaton package not found at $DIR/automaton" >&2
  exit 1
fi

export CLAWD_KIT_ROOT="$DIR"
export CLAWD_CONSTITUTION_PATH="${CLAWD_CONSTITUTION_PATH:-$DIR/automaton/constitution.md}"
export CLAWD_RULES_PATH="${CLAWD_RULES_PATH:-$DIR/automaton/scripts/clawd-rules.txt}"

# ── Install constitution + rules (immutable) ───────────────────────────────
STATE_DIR="${HOME:-/root}/.automaton"
CLAWD_STATE="${HOME:-/root}/.clawd"
mkdir -p "$STATE_DIR" "$CLAWD_STATE"

install_readonly() {
  src="$1"
  dst="$2"
  if [ -f "$src" ]; then
    # Prior installs are chmod 444; unlock before refresh
    if [ -e "$dst" ]; then
      chmod u+w "$dst" 2>/dev/null || true
      rm -f "$dst" 2>/dev/null || true
    fi
    cp "$src" "$dst"
    chmod 444 "$dst" 2>/dev/null || true
    echo "==> Installed $(basename "$dst") → $dst (read-only)"
  fi
}

install_readonly "$DIR/automaton/constitution.md" "$STATE_DIR/constitution.md"
install_readonly "$DIR/automaton/constitution.md" "$CLAWD_STATE/constitution.md"
install_readonly "$DIR/automaton/scripts/clawd-rules.txt" "$STATE_DIR/clawd-rules.txt"
install_readonly "$DIR/automaton/scripts/clawd-rules.txt" "$CLAWD_STATE/clawd-rules.txt"

# Persist kit root for agents
printf '%s\n' "$DIR" > "$STATE_DIR/kit_root"
printf '%s\n' "$DIR" > "$CLAWD_STATE/kit_root"
chmod 644 "$STATE_DIR/kit_root" "$CLAWD_STATE/kit_root" 2>/dev/null || true

# ── Build Rust kit (src → target) ─────────────────────────────────────────
build_kit() {
  echo "==> Building OpenClawd Solana kit (src/ → target/)..."
  cd "$DIR"
  if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: cargo not found — install Rust (https://rustup.rs) to build the kit" >&2
    return 1
  fi

  FEATURE_ARGS=""
  if [ -n "$KIT_FEATURES" ]; then
    FEATURE_ARGS="--features $KIT_FEATURES"
  fi

  if [ "$CARGO_PROFILE" = "release" ]; then
    # shellcheck disable=SC2086
    cargo build --release --manifest-path "$DIR/Cargo.toml" $FEATURE_ARGS
    # kit bin needs http feature
    # shellcheck disable=SC2086
    cargo build --release --manifest-path "$DIR/Cargo.toml" --features full --bin kit 2>/dev/null \
      || cargo build --release --manifest-path "$DIR/Cargo.toml" --bin kit $FEATURE_ARGS 2>/dev/null \
      || true
  else
    # shellcheck disable=SC2086
    cargo build --manifest-path "$DIR/Cargo.toml" $FEATURE_ARGS
    # shellcheck disable=SC2086
    cargo build --manifest-path "$DIR/Cargo.toml" --features full --bin kit 2>/dev/null \
      || cargo build --manifest-path "$DIR/Cargo.toml" --bin kit $FEATURE_ARGS 2>/dev/null \
      || true
  fi

  if [ -d "$DIR/target/debug" ] || [ -d "$DIR/target/release" ]; then
    echo "==> Kit build artifacts under $DIR/target/"
    ls -la "$DIR/target/debug/kit" "$DIR/target/release/kit" 2>/dev/null || true
    ls -la "$DIR/target/debug/libopenclawd_solana_kit"* 2>/dev/null | head -5 || true
  fi

  # Smoke: constitution unit tests (no network)
  echo "==> Verifying Clawd constitution load path (cargo test constitution)..."
  cargo test --manifest-path "$DIR/Cargo.toml" --lib constitution:: -- --nocapture
}

# ── Build TypeScript automaton ────────────────────────────────────────────
build_automaton() {
  echo "==> Building Clawd automaton (automaton/)..."
  cd "$DIR/automaton"
  if command -v pnpm >/dev/null 2>&1; then
    pnpm install && pnpm run build
  elif command -v npm >/dev/null 2>&1; then
    npm install && npm run build
  else
    echo "ERROR: need npm or pnpm for automaton" >&2
    return 1
  fi
}

case "$RUN_MODE" in
  kit)
    build_kit
    ;;
  automaton)
    build_automaton
    ;;
  both|*)
    build_kit
    build_automaton
    ;;
esac

if [ "$SKIP_START" = "1" ]; then
  echo "==> CLAWD_SKIP_START=1 — install/build complete; not starting runtime."
  echo "    Kit root: $DIR"
  echo "    Automaton: cd $DIR/automaton && node dist/index.js --run"
  echo "    Kit example: cd $DIR && cargo run --example solana_agent"
  echo "    Kit HTTP:    $DIR/target/${CARGO_PROFILE}/kit  (build with --features full)"
  exit 0
fi

# ── Start ─────────────────────────────────────────────────────────────────
case "$RUN_MODE" in
  kit)
    echo "==> Starting kit surface..."
    cd "$DIR"
    if [ -x "$DIR/target/$CARGO_PROFILE/kit" ]; then
      exec "$DIR/target/$CARGO_PROFILE/kit"
    fi
    echo "==> kit binary not present; running solana_agent example (needs SOLANA_PRIVATE_KEY)..."
    exec cargo run --manifest-path "$DIR/Cargo.toml" --example solana_agent
    ;;
  automaton)
    echo "==> Starting Clawd automaton..."
    cd "$DIR/automaton"
    exec node dist/index.js --run
    ;;
  both|*)
    echo "==> Starting Clawd automaton (kit available at CLAWD_KIT_ROOT=$DIR, target/)..."
    cd "$DIR/automaton"
    exec node dist/index.js --run
    ;;
esac
