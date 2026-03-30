#!/usr/bin/env node
"use strict";

const { execFileSync } = require("child_process");
const path = require("path");

const binaryName = process.platform === "win32" ? "cq.exe" : "cq";
const binary = path.join(__dirname, binaryName);

try {
  execFileSync(binary, process.argv.slice(2), { stdio: "inherit" });
} catch (e) {
  if (e.code === "ENOENT") {
    console.error(
      "cq binary not found. Run `npm rebuild cq` or reinstall the package."
    );
    process.exit(1);
  }
  process.exit(e.status || 1);
}
