#!/usr/bin/env node
/**
 * OpenClawd Solana Kit — one-shot CLI (npm / npx / curl-install wrapper).
 *
 * Usage:
 *   npx openclawd-solana-kit setup|doctor|check|build|start|example <name>
 *   npm run kit
 *
 * Env files (first wins for each key; existing process env always wins):
 *   CLAWD_ENV_FILE, .env, .env.local, src/.env.local
 * Docs: docs/configuration.md, docs/http_service.md, docs/quickstart.md
 */

import { spawn, spawnSync } from "node:child_process";
import {
  existsSync,
  copyFileSync,
  readFileSync,
  writeFileSync,
  mkdirSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PKG_ROOT = resolve(__dirname, "../..");

const ENV_CANDIDATES = [
  process.env.CLAWD_ENV_FILE,
  join(PKG_ROOT, ".env"),
  join(PKG_ROOT, ".env.local"),
  join(PKG_ROOT, "src/.env.local"),
  join(process.cwd(), ".env"),
  join(process.cwd(), ".env.local"),
  join(process.cwd(), "src/.env.local"),
].filter(Boolean);

const PRIVY_KEYS = ["PRIVY_APP_ID", "PRIVY_APP_SECRET", "PRIVY_VERIFICATION_KEY"];
const AGENT_KEYS = ["ANTHROPIC_API_KEY", "SOLANA_PRIVATE_KEY"];

function log(msg) {
  console.log(`openclawd-kit: ${msg}`);
}

function die(msg, code = 1) {
  console.error(`openclawd-kit: ${msg}`);
  process.exit(code);
}

/** Minimal .env parser (KEY=VALUE, strips quotes, ignores comments). */
function parseEnvFile(path) {
  const out = {};
  if (!existsSync(path)) return out;
  const text = readFileSync(path, "utf8");
  for (const line of text.split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    const i = t.indexOf("=");
    if (i <= 0) continue;
    const key = t.slice(0, i).trim();
    let val = t.slice(i + 1).trim();
    if (
      (val.startsWith('"') && val.endsWith('"')) ||
      (val.startsWith("'") && val.endsWith("'"))
    ) {
      val = val.slice(1, -1);
    }
    out[key] = val;
  }
  return out;
}

function loadEnvIntoProcess() {
  const loaded = [];
  const seen = new Set();
  for (const p of ENV_CANDIDATES) {
    const abs = resolve(p);
    if (!existsSync(abs) || seen.has(abs)) continue;
    seen.add(abs);
    const vars = parseEnvFile(abs);
    let n = 0;
    for (const [k, v] of Object.entries(vars)) {
      if (process.env[k] === undefined || process.env[k] === "") {
        process.env[k] = v;
        n++;
      }
    }
    loaded.push(`${abs} (+${n} keys)`);
  }
  return loaded;
}

function hasCargo() {
  return spawnSync("cargo", ["--version"], { encoding: "utf8" }).status === 0;
}

function hasRustc() {
  return spawnSync("rustc", ["--version"], { encoding: "utf8" }).status === 0;
}

function runCargo(args, { inherit = true } = {}) {
  if (!hasCargo()) {
    die(
      "cargo not found. Install Rust: https://rustup.rs\n  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    );
  }
  const env = { ...process.env };
  const r = spawnSync("cargo", args, {
    cwd: PKG_ROOT,
    env,
    stdio: inherit ? "inherit" : "pipe",
    encoding: "utf8",
  });
  if (r.status !== 0) {
    process.exit(r.status ?? 1);
  }
  return r;
}

function cmdHelp() {
  console.log(`
OpenClawd Solana Kit  ·  openclawd-solana-kit

  openclawd-kit setup              Copy .env.example → .env if missing
  openclawd-kit doctor             Check Rust + env readiness
  openclawd-kit check              cargo check (default solana)
  openclawd-kit build              cargo build --features full --bin kit
  openclawd-kit start              Run HTTP SSE on 0.0.0.0:6969
  openclawd-kit example simple     Run examples/simple.rs
  openclawd-kit example solana_agent

One-shot:
  npm install && npm run kit
  npx openclawd-solana-kit start
  curl -fsSL <raw>/scripts/install.sh | bash

Docs: docs/quickstart.md · docs/configuration.md · docs/http_service.md
Kit root: ${PKG_ROOT}
`);
}

function cmdSetup() {
  const example = join(PKG_ROOT, ".env.example");
  const dest = join(PKG_ROOT, ".env");
  if (!existsSync(example)) {
    die("missing .env.example in package root");
  }
  if (existsSync(dest)) {
    log(`.env already exists at ${dest}`);
  } else {
    copyFileSync(example, dest);
    log(`wrote ${dest} — fill PRIVY_* and ANTHROPIC_API_KEY (see docs/configuration.md)`);
  }
  const local = join(PKG_ROOT, "src/.env.local");
  if (existsSync(local)) {
    log(`also found ${local} (will be loaded automatically)`);
  }
  log("next: openclawd-kit doctor && openclawd-kit start");
}

function cmdDoctor() {
  const loaded = loadEnvIntoProcess();
  log(`kit root: ${PKG_ROOT}`);
  log(`rustc: ${hasRustc() ? "ok" : "MISSING"}`);
  log(`cargo: ${hasCargo() ? "ok" : "MISSING"}`);
  if (loaded.length) {
    log(`env files:\n  - ${loaded.join("\n  - ")}`);
  } else {
    log("env files: none found (run: openclawd-kit setup)");
  }

  const check = (keys, label) => {
    const miss = keys.filter((k) => !process.env[k] || !String(process.env[k]).trim());
    if (miss.length === 0) log(`${label}: ready`);
    else log(`${label}: missing ${miss.join(", ")}`);
    return miss.length === 0;
  };

  const privyOk = check(PRIVY_KEYS, "HTTP / Privy");
  check(AGENT_KEYS, "local examples (optional for HTTP)");
  if (process.env.SOLANA_RPC_URL) log(`SOLANA_RPC_URL: set`);
  else log(`SOLANA_RPC_URL: default will be public mainnet`);

  if (!privyOk) {
    console.log(`
Fix HTTP start:
  1. openclawd-kit setup
  2. Edit .env or src/.env.local with PRIVY_APP_ID, PRIVY_APP_SECRET, PRIVY_VERIFICATION_KEY
  3. See docs/authentication.md for the Bearer JWT frontend shape
  4. openclawd-kit start
`);
    process.exit(2);
  }
  log("doctor: OK — openclawd-kit start");
}

function cmdStart() {
  loadEnvIntoProcess();
  const miss = PRIVY_KEYS.filter((k) => !process.env[k]?.trim());
  if (miss.length) {
    die(
      `missing ${miss.join(", ")}.\nRun: openclawd-kit setup && edit .env\nDocs: docs/configuration.md`
    );
  }
  log("cargo run --features full --bin kit");
  runCargo(["run", "--features", "full", "--bin", "kit"]);
}

function cmdCheck() {
  loadEnvIntoProcess();
  runCargo(["check"]);
}

function cmdBuild() {
  loadEnvIntoProcess();
  runCargo(["build", "--features", "full", "--bin", "kit"]);
}

function cmdExample(name) {
  loadEnvIntoProcess();
  if (!name) die("usage: openclawd-kit example simple|solana_agent");
  const miss = ["ANTHROPIC_API_KEY", "SOLANA_PRIVATE_KEY"].filter(
    (k) => !process.env[k]?.trim()
  );
  if (miss.length) {
    die(`example needs ${miss.join(", ")} (docs/quickstart.md)`);
  }
  runCargo(["run", "--example", name]);
}

function cmdPostinstall() {
  // Soft hint only — do not fail npm install if cargo missing.
  if (!hasCargo()) {
    log("cargo not on PATH yet — install Rust then run: npx openclawd-solana-kit doctor");
    return;
  }
  log("installed. Try: npx openclawd-solana-kit doctor");
}

const [cmd, arg] = process.argv.slice(2);
switch (cmd) {
  case undefined:
  case "help":
  case "-h":
  case "--help":
    cmdHelp();
    break;
  case "setup":
    cmdSetup();
    break;
  case "doctor":
    cmdDoctor();
    break;
  case "check":
    cmdCheck();
    break;
  case "build":
    cmdBuild();
    break;
  case "start":
  case "kit":
  case "run":
    cmdStart();
    break;
  case "example":
    cmdExample(arg);
    break;
  case "postinstall":
    cmdPostinstall();
    break;
  default:
    die(`unknown command: ${cmd}\nRun: openclawd-kit help`);
}
