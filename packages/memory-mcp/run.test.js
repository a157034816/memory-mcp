'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const { resolvePlatformPackage } = require('./run.js');

test('resolvePlatformPackage() 映射已知平台', () => {
  assert.equal(resolvePlatformPackage('win32', 'x64'), '@a157034816/memory-mcp-win32-x64');
  assert.equal(resolvePlatformPackage('win32', 'arm64'), '@a157034816/memory-mcp-win32-arm64');
  assert.equal(resolvePlatformPackage('darwin', 'x64'), '@a157034816/memory-mcp-darwin-x64');
  assert.equal(resolvePlatformPackage('darwin', 'arm64'), '@a157034816/memory-mcp-darwin-arm64');
  assert.equal(resolvePlatformPackage('linux', 'x64'), '@a157034816/memory-mcp-linux-x64');
  assert.equal(resolvePlatformPackage('linux', 'arm64'), '@a157034816/memory-mcp-linux-arm64');
});

test('resolvePlatformPackage() 对未知平台返回 null', () => {
  assert.equal(resolvePlatformPackage('linux', 'ppc64'), null);
  assert.equal(resolvePlatformPackage('freebsd', 'x64'), null);
});
