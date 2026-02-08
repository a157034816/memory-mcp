#!/usr/bin/env node
'use strict';

const { spawn } = require('node:child_process');

function resolvePlatformPackage(platform, arch) {
  const key = `${platform}-${arch}`;
  switch (key) {
    case 'win32-x64':
      return '@a157034816/memory-mcp-win32-x64';
    case 'win32-arm64':
      return '@a157034816/memory-mcp-win32-arm64';
    case 'darwin-x64':
      return '@a157034816/memory-mcp-darwin-x64';
    case 'darwin-arm64':
      return '@a157034816/memory-mcp-darwin-arm64';
    case 'linux-x64':
      return '@a157034816/memory-mcp-linux-x64';
    case 'linux-arm64':
      return '@a157034816/memory-mcp-linux-arm64';
    default:
      return null;
  }
}

function loadBinaryPath(platformPackageName) {
  const mod = require(platformPackageName);
  if (!mod || typeof mod.binaryPath !== 'string' || mod.binaryPath.trim().length === 0) {
    throw new Error(`invalid platform package export: ${platformPackageName}`);
  }
  return mod.binaryPath;
}

function main(argv) {
  const platformPackageName = resolvePlatformPackage(process.platform, process.arch);
  if (!platformPackageName) {
    process.stderr.write(
      `不支持的平台：${process.platform} ${process.arch}\n` +
        '已支持：win32-x64, win32-arm64, darwin-x64, darwin-arm64, linux-x64, linux-arm64\n'
    );
    process.exit(1);
    return;
  }

  let binaryPath;
  try {
    binaryPath = loadBinaryPath(platformPackageName);
  } catch (err) {
    process.stderr.write(
      `无法加载平台二进制包：${platformPackageName}\n` +
        '这通常意味着 optionalDependencies 未正确安装（或被镜像/策略阻止）。\n' +
        `错误：${err && err.message ? err.message : String(err)}\n`
    );
    process.exit(1);
    return;
  }

  const child = spawn(binaryPath, argv, {
    stdio: 'inherit',
    env: process.env,
  });

  child.on('exit', (code, signal) => {
    if (signal) {
      process.exit(1);
      return;
    }
    process.exit(code ?? 1);
  });

  child.on('error', (err) => {
    process.stderr.write(
      `启动 Memory MCP 失败（${binaryPath}）：${err && err.message ? err.message : String(err)}\n`
    );
    process.exit(1);
  });
}

if (require.main === module) {
  main(process.argv.slice(2));
}

module.exports = {
  resolvePlatformPackage,
};
