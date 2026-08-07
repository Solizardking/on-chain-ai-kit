/**
 * System prompt builder — asserts Clawd constitution + CLAWD core rules ship
 * on the real buildSystemPrompt path (not Conway Cloud product identity).
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "fs";
import os from "os";
import path from "path";
import { buildSystemPrompt } from "../agent/system-prompt.js";
import { isProtectedFile } from "../self-mod/code.js";
import {
  createTestDb,
  createTestIdentity,
  createTestConfig,
} from "./mocks.js";
import type { AutomatonDatabase, FinancialState, AutomatonTool } from "../types.js";

describe("buildSystemPrompt — Clawd constitution & rules", () => {
  let db: AutomatonDatabase;
  let tmpHome: string;
  let prevHome: string | undefined;
  let prevCwd: string;

  beforeEach(() => {
    db = createTestDb();
    tmpHome = fs.mkdtempSync(path.join(os.tmpdir(), "clawd-prompt-"));
    prevHome = process.env.HOME;
    process.env.HOME = tmpHome;
    prevCwd = process.cwd();
    // Run from a cwd without constitution.md so we exercise the fallback path
    // when no file is installed; separately test file load when present.
    const emptyCwd = fs.mkdtempSync(path.join(os.tmpdir(), "clawd-cwd-"));
    process.chdir(emptyCwd);
  });

  afterEach(() => {
    process.chdir(prevCwd);
    if (prevHome === undefined) delete process.env.HOME;
    else process.env.HOME = prevHome;
    try {
      fs.rmSync(tmpHome, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  });

  const financial: FinancialState = {
    creditsCents: 500,
    usdcBalance: 1.5,
    lastChecked: new Date().toISOString(),
  };

  const tools: AutomatonTool[] = [
    {
      name: "exec",
      description: "Run a shell command",
      category: "vm",
      parameters: { type: "object", properties: {} },
      dangerous: false,
      execute: async () => "ok",
    },
  ];

  function prompt(opts?: { installConstitution?: boolean }): string {
    if (opts?.installConstitution) {
      const repoConst = path.join(prevCwd, "constitution.md");
      const destDir = path.join(tmpHome, ".automaton");
      fs.mkdirSync(destDir, { recursive: true });
      if (fs.existsSync(repoConst)) {
        fs.copyFileSync(repoConst, path.join(destDir, "constitution.md"));
      }
    }
    return buildSystemPrompt({
      identity: createTestIdentity(),
      config: createTestConfig(),
      financial,
      state: "running",
      db,
      tools,
      isFirstRun: false,
    });
  }

  it("includes CLAWD-branded core rules, not Conway Cloud product toolkit", () => {
    const p = prompt();
    expect(p).toMatch(/CLAWD Cloud/i);
    expect(p).toMatch(/CLAWD Compute/i);
    expect(p).toMatch(/CLAWD Domains/i);
    expect(p).toMatch(/CLAWD Terminal/i);
    expect(p).toMatch(/stablecoin wallet|Pay for compute or die/i);
    expect(p).not.toMatch(/Conway Cloud/);
    expect(p).not.toMatch(/Conway Domains/);
    expect(p).not.toMatch(/Conway Compute/);
    expect(p).not.toMatch(/Conway Terminal/);
  });

  it("embeds Clawd three-laws constitution (fallback when file missing)", () => {
    const p = prompt();
    expect(p).toMatch(/Law I/i);
    expect(p).toMatch(/Law II/i);
    expect(p).toMatch(/Law III/i);
    expect(p).toMatch(/Never harm/i);
    expect(p).toMatch(/Earn your existence/i);
    expect(p).toMatch(/Never deceive/i);
    expect(p).toMatch(/overrides/i);
    expect(p).toMatch(/shell molts/i);
    expect(p).toMatch(/beach/i);
    expect(p).toMatch(/CONSTITUTION/i);
  });

  it("loads shipped constitution.md from ~/.automaton when present", () => {
    const p = prompt({ installConstitution: true });
    expect(p).toMatch(/Clawd Constitution|Law I/i);
    expect(p).toMatch(/shell molts/i);
    expect(p).toMatch(/Inheritance|Propagated|child/i);
    expect(p).not.toMatch(/Conway Cloud/);
  });

  it("marks constitution block as immutable/protected in the prompt", () => {
    const p = prompt();
    expect(p).toMatch(/immutable|protected/i);
  });

  it("protects constitution and clawd-rules from self-modification", () => {
    expect(isProtectedFile("constitution.md")).toBe(true);
    expect(isProtectedFile("clawd-rules.txt")).toBe(true);
    expect(isProtectedFile("three-laws.md")).toBe(true);
    expect(isProtectedFile("SOUL.md")).toBe(false);
  });
});
