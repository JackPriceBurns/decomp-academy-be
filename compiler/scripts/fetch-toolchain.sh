#!/usr/bin/env bash
#
# Fetch the parts of the compile toolchain that are NOT committed to git, into
# compiler/vendor/, at build time:
#
#   * Metrowerks CodeWarrior mwcceppc.exe — one per version in compilers.list,
#     plus the shared license DLL. Proprietary; must NOT be redistributed, so they
#     are never committed here. Pulled from the decomp community compilers bundle
#     (tag-pinned), each verified against the SHA-1 in compilers.list.
#   * powerpc-eabi-objdump — GPL. Fetched from its upstream release so this repo
#     carries no GPL binary (and no accompanying-source obligation).
#
# wibo (MIT) stays committed under vendor/toolchain/ (see vendor/README.md).
#
# Always fetches the x86_64 Linux / PE32 binaries regardless of host OS — they
# run inside the x86_64 compile Lambda, never on the build host.
#
# Idempotent: a binary already present with the expected SHA-1 is left untouched,
# so warm CI caches and repeat local builds don't re-download the bundle.

set -euo pipefail

# Pinned upstream versions — bump together with the checksums below.
COMPILERS_TAG="20251118"   # https://files.decomp.dev/compilers_<tag>.zip
BINUTILS_TAG="2.42-1"      # https://github.com/encounter/gc-wii-binutils

# The license DLL (lmgr326b.dll for GC, lmgr8c.dll for Wii/3.0a) is one shared,
# byte-identical file across every version. Per-mwcceppc.exe SHA-1s live in
# compilers.list (the single source of truth, read below).
SHA_LMGR="0d2130e34e1651a7823ec57dc69cdf46fec563bd"
SHA_OBJDUMP="d3c465ca6ad3cd756600ee5ab133986070c50411"

# Resolve paths relative to this script so it works from any CWD (SAM runs the
# Makefile from compiler/; a dev may run it from the repo root).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILER_ROOT="$SCRIPT_DIR/.."
VENDOR="$COMPILER_ROOT/vendor"
COMPILER_DIR="$VENDOR/compiler"
BINUTILS_DIR="$VENDOR/toolchain/binutils"
MANIFEST="$COMPILER_ROOT/compilers.list"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

sha1() { shasum -a1 "$1" 2>/dev/null | awk '{print $1}'; }

# have <file> <expected-sha1>: true if the file exists and matches.
have() { [ -f "$1" ] && [ "$(sha1 "$1")" = "$2" ]; }

# require_sha <file> <expected-sha1> <label>: abort if the file doesn't match.
require_sha() {
  local got; got="$(sha1 "$1")"
  if [ "$got" != "$2" ]; then
    echo "ERROR: $3 SHA-1 mismatch — refusing to build with an unverified binary." >&2
    echo "  expected $2" >&2
    echo "  got      ${got:-<missing>}" >&2
    echo "  upstream changed under its pinned tag; review before bumping the checksum." >&2
    exit 1
  fi
}

# Emit "family|dir|sha1" for each manifest row (skips comments/blank lines).
manifest_rows() {
  grep -vE '^\s*(#|$)' "$MANIFEST" | awk -F'|' '{print $2"|"$3"|"$5}'
}

fetch_compilers() {
  local bundle_downloaded=0
  while IFS='|' read -r family dir sha; do
    local dest="$COMPILER_DIR/$family/$dir"
    if have "$dest/mwcceppc.exe" "$sha"; then
      continue
    fi
    if [ "$bundle_downloaded" -eq 0 ]; then
      local url="https://files.decomp.dev/compilers_${COMPILERS_TAG}.zip"
      echo "compilers: downloading $url"
      curl -fSL --retry 3 -o "$TMP/compilers.zip" "$url"
      bundle_downloaded=1
    fi
    echo "compilers: extracting $family/$dir"
    mkdir -p "$dest"
    # -j junks the archive prefix; -o overwrites. mwcceppc.exe + the dir's DLL(s).
    unzip -j -o "$TMP/compilers.zip" "$family/$dir/mwcceppc.exe" "$family/$dir/*.dll" -d "$dest" >/dev/null
    require_sha "$dest/mwcceppc.exe" "$sha" "$family/$dir/mwcceppc.exe"
    local lib
    for lib in "$dest"/*.dll; do
      require_sha "$lib" "$SHA_LMGR" "$(basename "$lib")"
    done
  done < <(manifest_rows)

  if [ "$bundle_downloaded" -eq 0 ]; then
    echo "compilers: all $(manifest_rows | wc -l | tr -d ' ') versions present, skipping"
  fi
}

fetch_objdump() {
  local out="$BINUTILS_DIR/powerpc-eabi-objdump"
  if have "$out" "$SHA_OBJDUMP"; then
    echo "objdump: present, skipping"
    return
  fi
  local url="https://github.com/encounter/gc-wii-binutils/releases/download/${BINUTILS_TAG}/linux-x86_64.zip"
  echo "objdump: downloading $url"
  curl -fSL --retry 3 -o "$TMP/binutils.zip" "$url"
  mkdir -p "$BINUTILS_DIR"
  unzip -j -o "$TMP/binutils.zip" "powerpc-eabi-objdump" -d "$BINUTILS_DIR"
  chmod +x "$out"
  require_sha "$out" "$SHA_OBJDUMP" "powerpc-eabi-objdump"
}

fetch_compilers
fetch_objdump
echo "toolchain ready."
