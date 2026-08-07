/**
 * Structural + real-entry smoke for the npm one-shot package.
 * Run: node --test npm/test/oneshot-package.test.mjs
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const BIN = join(ROOT, "npm/bin/openclawd-kit.mjs");

test("package.json has name, bin, files for one-shot", () => {
  const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
  assert.equal(pkg.name, "openclawd-solana-kit");
  assert.ok(pkg.bin["openclawd-kit"] || pkg.bin["openclawd-solana-kit"]);
  assert.ok(Array.isArray(pkg.files));
  assert.ok(pkg.files.some((f) => f.includes("npm")));
  assert.ok(pkg.files.some((f) => f.includes("frontend")));
  assert.ok(pkg.files.some((f) => f.includes("src")));
  assert.ok(pkg.scripts.start || pkg.scripts.kit);
});

test("CLI binary exists and --help mentions start", () => {
  assert.ok(existsSync(BIN), `missing ${BIN}`);
  const r = spawnSync(process.execPath, [BIN, "--help"], {
    encoding: "utf8",
    cwd: ROOT,
  });
  assert.equal(r.status, 0, r.stderr);
  const out = (r.stdout || "") + (r.stderr || "");
  assert.match(out, /start/i);
  assert.match(out, /doctor/i);
  assert.match(out, /openclawd/i);
});

test("frontend Agent Studio surfaces exist in package tree", () => {
  assert.ok(existsSync(join(ROOT, "frontend/index.html")));
  assert.ok(existsSync(join(ROOT, "frontend/chat.html")));
  const html = readFileSync(join(ROOT, "frontend/index.html"), "utf8");
  assert.ok(html.length > 100);
  assert.match(html, /Agent Studio|On-Chain AI|openclawd|Chat/i);
  const chat = readFileSync(join(ROOT, "frontend/chat.html"), "utf8");
  assert.match(chat, /\/stream|healthz/i);
});

test("Fly + Docker deploy artifacts exist", () => {
  assert.ok(existsSync(join(ROOT, "fly.toml")));
  assert.ok(existsSync(join(ROOT, "Dockerfile")));
  const fly = readFileSync(join(ROOT, "fly.toml"), "utf8");
  assert.match(fly, /openclawd-solana-kit/);
  assert.match(fly, /6969|healthz/);
  const df = readFileSync(join(ROOT, "Dockerfile"), "utf8");
  assert.match(df, /frontend/);
  assert.match(df, /openclawd-kit|bin\/kit/);
});

test("CLI resolves package root containing Cargo.toml and frontend", () => {
  // Drive real help path which prints Kit root
  const r = spawnSync(process.execPath, [BIN, "help"], {
    encoding: "utf8",
    cwd: ROOT,
  });
  assert.equal(r.status, 0);
  assert.match(r.stdout, /Kit root:/);
  assert.ok(existsSync(join(ROOT, "Cargo.toml")));
  assert.ok(existsSync(join(ROOT, "src/bin/kit.rs")));
});

test("npm pack tarball excludes secret env files", () => {
  const r = spawnSync("npm", ["pack", "--dry-run"], {
    encoding: "utf8",
    cwd: ROOT,
  });
  assert.equal(r.status, 0, r.stderr);
  const out = (r.stdout || "") + (r.stderr || "");
  // Must ship example template only
  assert.match(out, /\.env\.example/);
  // Must NOT ship local secrets
  assert.doesNotMatch(out, /src\/\.env\.local/);
  assert.doesNotMatch(out, /(?<!example)\.env(?!\.example)/);
  assert.doesNotMatch(out, /\.env\.local/);
  // One-shot surfaces still packed
  assert.match(out, /npm\/bin\/openclawd-kit\.mjs/);
  assert.match(out, /frontend\/index\.html/);
  assert.match(out, /frontend\/chat\.html/);
});
