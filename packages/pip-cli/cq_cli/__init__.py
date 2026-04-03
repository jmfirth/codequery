"""cq: semantic code query tool. 71 languages, three-tier precision."""

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
    """Return the cache directory for the cq binary."""
    if sys.platform == "win32":
        base = os.environ.get("LOCALAPPDATA", os.path.expanduser("~"))
        return Path(base) / "cq" / "bin"
    xdg = os.environ.get("XDG_DATA_HOME")
    if xdg:
        return Path(xdg) / "cq" / "bin"
    return Path.home() / ".local" / "share" / "cq" / "bin"


def _get_binary_name():
    """Return the platform-appropriate binary name."""
    return "cq.exe" if sys.platform == "win32" else "cq"


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
            f"Install cq manually from https://github.com/{REPO}/releases"
        )
    target, ext = info
    filename = f"codequery-v{__version__}-{target}.{ext}"
    url = f"https://github.com/{REPO}/releases/download/v{__version__}/{filename}"
    return url, ext


def _download(url):
    """Download a URL, following redirects. Returns bytes."""
    # urllib handles redirects automatically
    req = urllib.request.Request(url, headers={"User-Agent": "cq-installer"})
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


def _ensure_binary():
    """Ensure the cq binary is installed and return its path."""
    cache_dir = _get_cache_dir()
    binary_name = _get_binary_name()
    binary_path = cache_dir / binary_name

    if binary_path.exists():
        return str(binary_path)

    url, ext = _get_artifact_info()
    plat_key = _get_platform_key()

    print(f"Downloading cq v{__version__} for {plat_key[0]}/{plat_key[1]}...",
          file=sys.stderr)
    print(f"  {url}", file=sys.stderr)

    data = _download(url)

    cache_dir.mkdir(parents=True, exist_ok=True)

    if ext == "tar.gz":
        result = _extract_tar_gz(data, str(cache_dir), binary_name)
    else:
        result = _extract_zip(data, str(cache_dir), binary_name)

    # Make executable on Unix
    if sys.platform != "win32":
        st = os.stat(result)
        os.chmod(result, st.st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)

    print(f"Installed cq to {result}", file=sys.stderr)
    return str(result)


def main():
    """Entry point: ensure binary exists, then exec it."""
    try:
        binary = _ensure_binary()
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    args = sys.argv[1:]

    if sys.platform != "win32":
        # Replace the current process on Unix
        os.execv(binary, [binary] + args)
    else:
        # On Windows, use subprocess
        result = subprocess.run([binary] + args)
        sys.exit(result.returncode)
