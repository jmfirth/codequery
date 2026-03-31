#!/usr/bin/env node
"use strict";

const { execFileSync } = require("child_process");
const path = require("path");

const binaryName = process.platform === "win32" ? "cq-mcp.exe" : "cq-mcp";
const cqName = process.platform === "win32" ? "cq.exe" : "cq";
const binary = path.join(__dirname, binaryName);
const cqBinary = path.join(__dirname, cqName);

try {
  // Set CQ_BIN so cq-mcp can find the co-installed cq binary
  const env = { ...process.env };
  if (!env.CQ_BIN && require("fs").existsSync(cqBinary)) {
    env.CQ_BIN = cqBinary;
  }
  execFileSync(binary, process.argv.slice(2), { stdio: "inherit", env });
} catch (e) {
  if (e.code === "ENOENT") {
    console.error(
      "cq-mcp binary not found. Run `npm rebuild cq-mcp` or reinstall the package."
    );
    process.exit(1);
  }
  process.exit(e.status || 1);
}
