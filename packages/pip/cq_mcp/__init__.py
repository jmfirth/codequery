"""cq-mcp: MCP server for cq, a semantic code query tool."""

import os
import platform
import stat
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
from io import BytesIO
from pathlib import Path

__version__ = "1.0.0"

REPO = "jmfirth/codequery"

PLATFORM_MAP = {
    ("darwin", "arm64"): ("aarch64-apple-darwin", "tar.gz"),
    ("darwin", "x86_64"): ("x86_64-apple-darwin", "tar.gz"),
    ("linux", "x86_64"): ("x86_64-unknown-linux-gnu", "tar.gz"),
    ("linux", "aarch64"): ("aarch64-unknown-linux-gnu", "tar.gz"),
    ("win32", "AMD64"): ("x86_64-pc-windows-msvc", "zip"),
}


def _get_cache_dir():
    """Return the cache directory for the cq-mcp binary."""
    if sys.platform == "win32":
        base = os.environ.get("LOCALAPPDATA", os.path.expanduser("~"))
        return Path(base) / "cq" / "bin"
    xdg = os.environ.get("XDG_DATA_HOME")
    if xdg:
        return Path(xdg) / "cq" / "bin"
    return Path.home() / ".local" / "share" / "cq" / "bin"


def _get_binary_name():
    """Return the platform-appropriate binary name."""
    return "cq-mcp.exe" if sys.platform == "win32" else "cq-mcp"


def _get_platform_key():
    """Return (platform, machine) tuple normalized for lookup."""
    plat = sys.platform
    machine = platform.machine()
    # Normalize: some systems report 'x86_64' as 'x86_64', arm64 as 'arm64'
    return (plat, machine)


def _get_artifact_info():
    """Return (url, extension) for the current platform."""
    key = _get_platform_key()
    info = PLATFORM_MAP.get(key)
    if info is None:
        raise RuntimeError(
            f"Unsupported platform: {key[0]}/{key[1]}\n"
            f"Supported platforms: {list(PLATFORM_MAP.keys())}\n"
            f"Install cq-mcp manually from https://github.com/{REPO}/releases"
        )
    target, ext = info
    filename = f"cq-mcp-{target}.{ext}"
    url = f"https://github.com/{REPO}/releases/download/v{__version__}/{filename}"
    return url, ext


def _download(url):
    """Download a URL, following redirects. Returns bytes."""
    # urllib handles redirects automatically
    req = urllib.request.Request(url, headers={"User-Agent": "cq-mcp-installer"})
    try:
        with urllib.request.urlopen(req) as resp:
            return resp.read()
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"Download failed: HTTP {e.code} for {url}") from e
    except urllib.error.URLError as e:
        raise RuntimeError(f"Download failed: {e.reason} for {url}") from e


def _extract_tar_gz(data, dest_dir, binary_name):
    """Extract the binary from a tar.gz archive."""
    buf = BytesIO(data)
    with tarfile.open(fileobj=buf, mode="r:gz") as tf:
        for member in tf.getmembers():
            if member.name.endswith(binary_name) and member.isfile():
                member.name = binary_name
                tf.extract(member, dest_dir)
                return Path(dest_dir) / binary_name
    raise RuntimeError(f"Could not find {binary_name} in archive")


def _extract_zip(data, dest_dir, binary_name):
    """Extract the binary from a zip archive."""
    buf = BytesIO(data)
    with zipfile.ZipFile(buf) as zf:
        for name in zf.namelist():
            if name.endswith(binary_name):
                # Extract just this file to dest_dir with flat name
                info = zf.getinfo(name)
                info.filename = binary_name
                zf.extract(info, dest_dir)
                return Path(dest_dir) / binary_name
    raise RuntimeError(f"Could not find {binary_name} in archive")


def _download_binary(cache_dir, binary_name, url, ext):
    """Download, extract, and install a single binary. Returns path."""
    data = _download(url)
    cache_dir.mkdir(parents=True, exist_ok=True)

    if ext == "tar.gz":
        result = _extract_tar_gz(data, str(cache_dir), binary_name)
    else:
        result = _extract_zip(data, str(cache_dir), binary_name)

    if sys.platform != "win32":
        st = os.stat(result)
        os.chmod(result, st.st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)

    return result


def _ensure_binary():
    """Ensure cq-mcp and cq binaries are installed. Returns cq-mcp path."""
    cache_dir = _get_cache_dir()
    mcp_name = _get_binary_name()
    cq_name = "cq.exe" if sys.platform == "win32" else "cq"
    mcp_path = cache_dir / mcp_name
    cq_path = cache_dir / cq_name

    if mcp_path.exists() and cq_path.exists():
        return str(mcp_path)

    plat_key = _get_platform_key()
    info = PLATFORM_MAP.get(plat_key)
    if info is None:
        raise RuntimeError(f"Unsupported platform: {plat_key[0]}/{plat_key[1]}")
    target, ext = info

    if not mcp_path.exists():
        mcp_url = f"https://github.com/{REPO}/releases/download/v{__version__}/cq-mcp-{target}.{ext}"
        print(f"Downloading cq-mcp v{__version__} for {plat_key[0]}/{plat_key[1]}...",
              file=sys.stderr)
        result = _download_binary(cache_dir, mcp_name, mcp_url, ext)
        print(f"Installed cq-mcp to {result}", file=sys.stderr)

    if not cq_path.exists():
        cq_url = f"https://github.com/{REPO}/releases/download/v{__version__}/codequery-v{__version__}-{target}.{ext}"
        print(f"Downloading cq v{__version__}...", file=sys.stderr)
        try:
            result = _download_binary(cache_dir, cq_name, cq_url, ext)
            print(f"Installed cq to {result}", file=sys.stderr)
        except RuntimeError as e:
            print(f"Warning: could not download cq ({e}). cq-mcp will look for cq on PATH.",
                  file=sys.stderr)

    return str(mcp_path)


def main():
    """Entry point: ensure binary exists, then exec it."""
    try:
        binary = _ensure_binary()
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    # Set CQ_BIN so cq-mcp can find the co-installed cq binary
    cache_dir = _get_cache_dir()
    cq_name = "cq.exe" if sys.platform == "win32" else "cq"
    cq_path = cache_dir / cq_name
    if not os.environ.get("CQ_BIN") and cq_path.exists():
        os.environ["CQ_BIN"] = str(cq_path)

    args = sys.argv[1:]

    if sys.platform != "win32":
        os.execv(binary, [binary] + args)
    else:
        result = subprocess.run([binary] + args)
        sys.exit(result.returncode)
