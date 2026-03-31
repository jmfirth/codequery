"use strict";

const https = require("https");
const http = require("http");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const os = require("os");

const VERSION = require("./package.json").version;
const REPO = "jmfirth/codequery";

const PLATFORM_MAP = {
  "darwin-arm64": { target: "aarch64-apple-darwin", ext: "tar.gz" },
  "darwin-x64": { target: "x86_64-apple-darwin", ext: "tar.gz" },
  "linux-x64": { target: "x86_64-unknown-linux-gnu", ext: "tar.gz" },
  "linux-arm64": { target: "aarch64-unknown-linux-gnu", ext: "tar.gz" },
  "win32-x64": { target: "x86_64-pc-windows-msvc", ext: "zip" },
};

function getPlatformKey() {
  return `${process.platform}-${process.arch}`;
}

function getArtifactInfo() {
  const key = getPlatformKey();
  const info = PLATFORM_MAP[key];
  if (!info) {
    throw new Error(
      `Unsupported platform: ${key}\n` +
        `Supported platforms: ${Object.keys(PLATFORM_MAP).join(", ")}\n` +
        `Install cq-mcp manually from https://github.com/${REPO}/releases`
    );
  }
  const filename = `cq-mcp-${info.target}.${info.ext}`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${filename}`;
  return { url, filename, ext: info.ext };
}

function download(url) {
  return new Promise((resolve, reject) => {
    const get = url.startsWith("https:") ? https.get : http.get;
    get(url, (res) => {
      // Follow redirects (GitHub releases return 302 to S3)
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        return download(res.headers.location).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        reject(new Error(`Download failed: HTTP ${res.statusCode} for ${url}`));
        return;
      }
      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => resolve(Buffer.concat(chunks)));
      res.on("error", reject);
    }).on("error", reject);
  });
}

function extractTarGz(buffer, destDir) {
  const tmpFile = path.join(os.tmpdir(), `cq-mcp-${Date.now()}.tar.gz`);
  fs.writeFileSync(tmpFile, buffer);
  try {
    execSync(`tar xzf "${tmpFile}" -C "${destDir}" --strip-components=0`, {
      stdio: "pipe",
    });
  } finally {
    try {
      fs.unlinkSync(tmpFile);
    } catch (_) {
      // ignore cleanup errors
    }
  }
}

function extractZip(buffer, destDir) {
  const tmpFile = path.join(os.tmpdir(), `cq-mcp-${Date.now()}.zip`);
  fs.writeFileSync(tmpFile, buffer);
  try {
    if (process.platform === "win32") {
      execSync(
        `powershell -Command "Expand-Archive -Path '${tmpFile}' -DestinationPath '${destDir}' -Force"`,
        { stdio: "pipe" }
      );
    } else {
      execSync(`unzip -o "${tmpFile}" -d "${destDir}"`, { stdio: "pipe" });
    }
  } finally {
    try {
      fs.unlinkSync(tmpFile);
    } catch (_) {
      // ignore cleanup errors
    }
  }
}

async function downloadAndExtract(url, ext, binDir, binaryName) {
  let buffer;
  try {
    buffer = await download(url);
  } catch (e) {
    throw new Error(`Failed to download ${binaryName}: ${e.message}`);
  }

  const tmpDir = path.join(os.tmpdir(), `cq-extract-${Date.now()}`);
  fs.mkdirSync(tmpDir, { recursive: true });

  try {
    if (ext === "tar.gz") {
      extractTarGz(buffer, tmpDir);
    } else {
      extractZip(buffer, tmpDir);
    }

    const extracted = findBinary(tmpDir, binaryName);
    if (!extracted) {
      throw new Error(
        `Could not find ${binaryName} in downloaded archive. ` +
          `Contents: ${listDir(tmpDir).join(", ")}`
      );
    }

    fs.mkdirSync(binDir, { recursive: true });
    const dest = path.join(binDir, binaryName);
    fs.copyFileSync(extracted, dest);

    if (process.platform !== "win32") {
      fs.chmodSync(dest, 0o755);
    }

    return dest;
  } finally {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch (_) {
      // ignore
    }
  }
}

async function main() {
  const binDir = path.join(__dirname, "bin");
  const mcpName = process.platform === "win32" ? "cq-mcp.exe" : "cq-mcp";
  const cqName = process.platform === "win32" ? "cq.exe" : "cq";

  // Skip if both binaries already exist
  if (fs.existsSync(path.join(binDir, mcpName)) && fs.existsSync(path.join(binDir, cqName))) {
    console.log(`cq-mcp and cq binaries already exist in ${binDir}`);
    return;
  }

  let artifact;
  try {
    artifact = getArtifactInfo();
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }

  const key = getPlatformKey();
  const info = PLATFORM_MAP[key];

  // Download cq-mcp
  console.log(`Downloading cq-mcp v${VERSION} for ${key}...`);
  console.log(`  ${artifact.url}`);
  try {
    const dest = await downloadAndExtract(artifact.url, artifact.ext, binDir, mcpName);
    console.log(`Installed cq-mcp to ${dest}`);
  } catch (e) {
    console.error(
      `\n${e.message}\n\n` +
        `Install manually from: https://github.com/${REPO}/releases/tag/v${VERSION}\n`
    );
    process.exit(1);
  }

  // Download cq (cq-mcp needs it to run queries)
  const cqFilename = `codequery-v${VERSION}-${info.target}.${info.ext}`;
  const cqUrl = `https://github.com/${REPO}/releases/download/v${VERSION}/${cqFilename}`;
  console.log(`Downloading cq v${VERSION}...`);
  console.log(`  ${cqUrl}`);
  try {
    const dest = await downloadAndExtract(cqUrl, info.ext, binDir, cqName);
    console.log(`Installed cq to ${dest}`);
  } catch (e) {
    console.warn(`Warning: could not download cq binary (${e.message})`);
    console.warn(`cq-mcp will look for cq on PATH. Install cq separately if needed.`);
  }
}

function findBinary(dir, name) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isFile() && entry.name === name) {
      return full;
    }
    if (entry.isDirectory()) {
      const found = findBinary(full, name);
      if (found) return found;
    }
  }
  return null;
}

function listDir(dir) {
  const result = [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    result.push(entry.name);
    if (entry.isDirectory()) {
      result.push(...listDir(full).map((f) => `${entry.name}/${f}`));
    }
  }
  return result;
}

main().catch((e) => {
  console.error(`Unexpected error during cq-mcp installation: ${e.message}`);
  process.exit(1);
});
