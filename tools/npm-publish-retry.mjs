#!/usr/bin/env node
/**
 * npm publish 的“幂等 + 重试”封装：
 * - 先用 `npm view name@version` 判断是否已发布（已发布则跳过并视为成功）
 * - 发布遇到 npm registry 短暂处理延迟（如 E409 / packument）或网络抖动时自动退避重试
 * - 发布成功后等待版本在 registry 可见（降低“上一个包未处理完”导致的连锁失败）
 *
 * 设计目标：让 CI 发布尽量可恢复（可重跑），并在失败时输出足够的诊断信息。
 */

import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const NPM_CMD = process.platform === "win32" ? "npm.cmd" : "npm";
const DEFAULT_BUDGET_SECONDS = 600;
const DEFAULT_POLL_SECONDS = 120;
const VIEW_POLL_INTERVAL_MS = 5000;

function printUsage(exitCode) {
  const msg = `
用法:
  node tools/npm-publish-retry.mjs <packageDir> [--budget-seconds 600] [--poll-seconds 120]

参数:
  <packageDir>         npm 包目录（包含 package.json）
  --budget-seconds N   单个包发布的总重试预算（秒）
  --poll-seconds N     publish 后等待 registry 可见性的最长时间（秒）
`.trim();
  if (exitCode === 0) console.log(msg);
  else console.error(msg);
  process.exit(exitCode);
}

function parseArgs(argv) {
  let packageDir = null;
  let budgetSeconds = DEFAULT_BUDGET_SECONDS;
  let pollSeconds = DEFAULT_POLL_SECONDS;

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];

    if (arg === "--help" || arg === "-h") printUsage(0);

    if (arg === "--budget-seconds") {
      const value = argv[i + 1];
      if (!value) printUsage(1);
      budgetSeconds = Number(value);
      i += 1;
      continue;
    }

    if (arg === "--poll-seconds") {
      const value = argv[i + 1];
      if (!value) printUsage(1);
      pollSeconds = Number(value);
      i += 1;
      continue;
    }

    if (arg.startsWith("--")) {
      console.error(`未知参数: ${arg}`);
      printUsage(1);
    }

    if (!packageDir) {
      packageDir = arg;
      continue;
    }

    console.error(`多余参数: ${arg}`);
    printUsage(1);
  }

  if (!packageDir) printUsage(1);
  if (!Number.isFinite(budgetSeconds) || budgetSeconds <= 0) {
    console.error(`--budget-seconds 必须是正数，收到: ${budgetSeconds}`);
    process.exit(1);
  }
  if (!Number.isFinite(pollSeconds) || pollSeconds <= 0) {
    console.error(`--poll-seconds 必须是正数，收到: ${pollSeconds}`);
    process.exit(1);
  }

  return { packageDir, budgetSeconds, pollSeconds };
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function nowIso() {
  return new Date().toISOString();
}

function logInfo(message) {
  console.log(`[${nowIso()}] ${message}`);
}

function logWarn(message) {
  console.warn(`[${nowIso()}] WARN: ${message}`);
}

function logError(message) {
  console.error(`[${nowIso()}] ERROR: ${message}`);
}

function getRegistry() {
  return (
    process.env.npm_config_registry ||
    process.env.NPM_CONFIG_REGISTRY ||
    "https://registry.npmjs.org"
  );
}

async function runCommand(command, args, { cwd, streamOutput = true }) {
  return await new Promise((resolve) => {
    const child = spawn(command, args, {
      cwd,
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });

    let stdout = "";
    let stderr = "";
    let settled = false;

    child.stdout.on("data", (chunk) => {
      const text = chunk.toString();
      stdout += text;
      if (streamOutput) process.stdout.write(text);
    });
    child.stderr.on("data", (chunk) => {
      const text = chunk.toString();
      stderr += text;
      if (streamOutput) process.stderr.write(text);
    });

    child.on("error", (err) => {
      if (settled) return;
      settled = true;
      const message = err && err.message ? err.message : String(err);
      const combined = `${stdout}\n${stderr}\n${message}`.trim();
      resolve({
        exitCode: 1,
        signal: null,
        stdout,
        stderr: `${stderr}\n${message}`.trim(),
        combined,
      });
    });

    child.on("close", (code, signal) => {
      if (settled) return;
      settled = true;
      const exitCode = typeof code === "number" ? code : 1;
      resolve({
        exitCode,
        signal: signal || null,
        stdout,
        stderr,
        combined: `${stdout}\n${stderr}`.trim(),
      });
    });
  });
}

async function isVersionPublished({ name, version, cwd, registry }) {
  const result = await runCommand(
    NPM_CMD,
    ["view", `${name}@${version}`, "version", "--silent", "--registry", registry],
    { cwd, streamOutput: false }
  );
  return result.exitCode === 0;
}

function classifyPublishFailure(output) {
  const alreadyPublishedPatterns = [
    /EPUBLISHCONFLICT/i,
    /cannot publish over/i,
    /previously published version/i,
    /previously published versions/i,
  ];
  if (alreadyPublishedPatterns.some((re) => re.test(output))) return "already_published";

  const retryablePatterns = [
    /\bE409\b/i,
    /\b409\b.*\bConflict\b/i,
    /Failed to save packument/i,
    /previous package has been fully processed/i,
    /\bETIMEDOUT\b/i,
    /\bECONNRESET\b/i,
    /\bEAI_AGAIN\b/i,
    /socket hang up/i,
    /\b502\b|\b503\b|\b504\b/i,
    /bad gateway/i,
    /service unavailable/i,
    /gateway timeout/i,
  ];
  if (retryablePatterns.some((re) => re.test(output))) return "retryable";

  const fatalPatterns = [
    /\bE401\b/i,
    /\b401\b.*\bUnauthorized\b/i,
    /\bE403\b/i,
    /\b403\b.*\bForbidden\b/i,
    /\bE402\b/i,
    /\bPayment Required\b/i,
    /\bENEEDAUTH\b/i,
  ];
  if (fatalPatterns.some((re) => re.test(output))) return "fatal";

  return "fatal";
}

async function waitUntilVisible({ name, version, cwd, registry, timeoutMs }) {
  const endAt = Date.now() + timeoutMs;
  let nextLogAt = 0;

  logInfo(`等待 registry 可见性: ${name}@${version}（最多 ${Math.ceil(timeoutMs / 1000)}s）`);

  while (Date.now() < endAt) {
    const ok = await isVersionPublished({ name, version, cwd, registry });
    if (ok) {
      logInfo(`可见性确认 OK: ${name}@${version}`);
      return true;
    }

    const now = Date.now();
    if (now >= nextLogAt) {
      const remaining = Math.max(0, Math.ceil((endAt - now) / 1000));
      logInfo(`仍未可见，继续等待...（剩余 ~${remaining}s）`);
      nextLogAt = now + 15000;
    }

    await sleep(Math.min(VIEW_POLL_INTERVAL_MS, endAt - Date.now()));
  }

  logWarn(`可见性确认超时: ${name}@${version}`);
  return false;
}

async function main() {
  const { packageDir, budgetSeconds, pollSeconds } = parseArgs(process.argv.slice(2));
  const cwd = path.resolve(packageDir);
  const registry = getRegistry();

  const pkgPath = path.join(cwd, "package.json");
  const pkg = JSON.parse(await fs.readFile(pkgPath, "utf8"));
  const name = pkg.name;
  const version = pkg.version;

  if (!name || !version) {
    logError(`package.json 缺少 name/version: ${pkgPath}`);
    process.exit(1);
  }

  const deadline = Date.now() + budgetSeconds * 1000;
  let attempt = 0;
  let delayMs = 5000;

  logInfo(`开始发布: ${name}@${version}`);
  logInfo(`registry: ${registry}`);

  while (Date.now() < deadline) {
    // 幂等：重跑 workflow 时，如果同版本已发布，直接跳过并视为成功。
    if (await isVersionPublished({ name, version, cwd, registry })) {
      logInfo(`SKIP 已发布: ${name}@${version}`);
      return;
    }

    attempt += 1;
    logInfo(`publish 尝试 #${attempt}: ${name}@${version}`);

    const publishResult = await runCommand(
      NPM_CMD,
      ["publish", "--access", "public", "--registry", registry],
      { cwd }
    );

    const remainingAfterPublishMs = Math.max(0, deadline - Date.now());
    const pollWindowMs = Math.min(pollSeconds * 1000, remainingAfterPublishMs);

    if (publishResult.exitCode === 0) {
      const visible = await waitUntilVisible({
        name,
        version,
        cwd,
        registry,
        timeoutMs: pollWindowMs,
      });
      if (visible) return;
      logWarn("publish 已成功，但版本暂未可见，将在预算内继续重试/等待。");
    } else {
      const classification = classifyPublishFailure(publishResult.combined);

      if (classification === "already_published") {
        logInfo("检测到已发布冲突（EPUBLISHCONFLICT/重复版本），按幂等视为成功并等待可见性。");
        const visible = await waitUntilVisible({
          name,
          version,
          cwd,
          registry,
          timeoutMs: pollWindowMs,
        });
        if (visible) return;
      } else if (classification === "retryable") {
        logWarn("检测到可重试错误（registry 延迟/网络抖动），将在预算内退避重试。");
      } else {
        logError(`不可重试的发布失败: ${name}@${version}`);
        if (publishResult.signal) logError(`signal: ${publishResult.signal}`);
        process.exit(1);
      }
    }

    const remainingMs = deadline - Date.now();
    if (remainingMs <= 0) break;

    const jitterMs = Math.floor(Math.random() * 3000);
    const sleepMs = Math.min(delayMs + jitterMs, remainingMs);
    logInfo(`等待 ${Math.ceil(sleepMs / 1000)}s 后重试...`);
    await sleep(sleepMs);
    delayMs = Math.min(delayMs * 2, 60000);
  }

  logError(`发布超时（预算 ${budgetSeconds}s）：${name}@${version}`);
  process.exit(1);
}

await main();
