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
#   * IDO 5.3 cc + its compiler passes (N64/MIPS course) — SGI proprietary code
#     statically recompiled by decompals/ido-static-recomp; fetched from that
#     project's release, never committed.
#   * mips objdump (GPL, decompals binutils build) + the libzstd.so.1 it links
#     (BSD, Debian bullseye via the immutable snapshot.debian.org archive) —
#     fetched, never committed, same reasoning as powerpc-eabi-objdump.
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
IDO_RECOMP_TAG="v1.2"      # https://github.com/decompals/ido-static-recomp
# decompals/binutils-mips-ps2-decompals (binutils 2.40). v0.4 is the newest
# release whose linux build still runs on AL2023: it needs glibc <= 2.34 (the
# Lambda runtime's exact version); v0.5+ were built against glibc 2.38.
MIPS_BINUTILS_TAG="v0.4"

# The license DLL (lmgr326b.dll for GC, lmgr8c.dll for Wii/3.0a) is one shared,
# byte-identical file across every version. Per-mwcceppc.exe SHA-1s live in
# compilers.list (the single source of truth, read below).
SHA_LMGR="0d2130e34e1651a7823ec57dc69cdf46fec563bd"
SHA_OBJDUMP="d3c465ca6ad3cd756600ee5ab133986070c50411"

# IDO 5.3: the whole linux tarball is vendored (cc is a driver that execs its
# sibling passes cfe/uopt/ugen/as1 from its own directory); the extracted cc's
# SHA doubles as the idempotency probe.
SHA_IDO_TARBALL="976b115acb973c3828a7215b531b203537135e38"
SHA_IDO_CC="122e049ec39445b24777543f56b667520400fbca"

# mips objdump + its one runtime dep. The objdump is dynamically linked against
# libzstd.so.1, which the provided.al2023 runtime may not ship — so the exact
# bullseye libzstd (glibc floor 2.14) is vendored beside it and the service sets
# LD_LIBRARY_PATH for the objdump call. The deb URL is content-addressed by its
# own SHA-1 (snapshot.debian.org/file/<sha1>), so the pin IS the URL.
SHA_MIPS_BINUTILS_TARBALL="1b416ca371f7555d438d86d7084ec1afebe8fc84"
SHA_MIPS_OBJDUMP="8a456ec264359ba8413569a4c7f30511097b6d83"
SHA_LIBZSTD_DEB="649d60bc71ea1c64c7cc1e021698fcbf6d3de8cc"   # libzstd1_1.4.8+dfsg-2.1_amd64.deb
SHA_LIBZSTD_SO="a5826880db6f1b81558c9ba463b2cebee80d76eb"

# Resolve paths relative to this script so it works from any CWD (SAM runs the
# Makefile from compiler/; a dev may run it from the repo root).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILER_ROOT="$SCRIPT_DIR/.."
VENDOR="$COMPILER_ROOT/vendor"
COMPILER_DIR="$VENDOR/compiler"
BINUTILS_DIR="$VENDOR/toolchain/binutils"
IDO_DIR="$VENDOR/ido/5.3"
BINUTILS_MIPS_DIR="$VENDOR/toolchain/binutils-mips"
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

fetch_ido() {
  if have "$IDO_DIR/cc" "$SHA_IDO_CC"; then
    echo "ido 5.3: present, skipping"
    return
  fi
  local url="https://github.com/decompals/ido-static-recomp/releases/download/${IDO_RECOMP_TAG}/ido-5.3-recomp-linux.tar.gz"
  echo "ido 5.3: downloading $url"
  curl -fSL --retry 3 -o "$TMP/ido53.tar.gz" "$url"
  require_sha "$TMP/ido53.tar.gz" "$SHA_IDO_TARBALL" "ido-5.3-recomp-linux.tar.gz"
  mkdir -p "$IDO_DIR"
  # Flat tarball: cc, cfe, uopt, ugen, as0/as1, acpp, err.english.cc, libc.so…
  # All of it ships — cc execs the passes from its own directory.
  tar -xzf "$TMP/ido53.tar.gz" -C "$IDO_DIR"
  require_sha "$IDO_DIR/cc" "$SHA_IDO_CC" "ido/5.3/cc"
}

fetch_mips_objdump() {
  local out="$BINUTILS_MIPS_DIR/mips-ps2-decompals-objdump"
  local lib="$BINUTILS_MIPS_DIR/lib/libzstd.so.1"
  if have "$out" "$SHA_MIPS_OBJDUMP" && have "$lib" "$SHA_LIBZSTD_SO"; then
    echo "mips objdump: present, skipping"
    return
  fi
  local url="https://github.com/decompals/binutils-mips-ps2-decompals/releases/download/${MIPS_BINUTILS_TAG}/binutils-mips-ps2-decompals-linux-x86-64.tar.gz"
  echo "mips objdump: downloading $url"
  curl -fSL --retry 3 -o "$TMP/binutils-mips.tar.gz" "$url"
  require_sha "$TMP/binutils-mips.tar.gz" "$SHA_MIPS_BINUTILS_TARBALL" "binutils-mips tarball"
  mkdir -p "$TMP/binutils-mips" "$BINUTILS_MIPS_DIR/lib"
  tar -xzf "$TMP/binutils-mips.tar.gz" -C "$TMP/binutils-mips"
  cp "$TMP/binutils-mips/mips-ps2-decompals-objdump" "$out"
  chmod +x "$out"
  require_sha "$out" "$SHA_MIPS_OBJDUMP" "mips-ps2-decompals-objdump"

  local zurl="https://snapshot.debian.org/file/${SHA_LIBZSTD_DEB}"
  echo "libzstd: downloading $zurl"
  curl -fSL --retry 3 -o "$TMP/libzstd1.deb" "$zurl"
  require_sha "$TMP/libzstd1.deb" "$SHA_LIBZSTD_DEB" "libzstd1 deb"
  # A .deb is an ar archive wrapping data.tar.xz (works with BSD and GNU ar).
  mkdir -p "$TMP/libzstd"
  (cd "$TMP/libzstd" && ar x "$TMP/libzstd1.deb" data.tar.xz && tar -xf data.tar.xz)
  cp "$TMP/libzstd/usr/lib/x86_64-linux-gnu/libzstd.so.1.4.8" "$lib"
  require_sha "$lib" "$SHA_LIBZSTD_SO" "libzstd.so.1"
}

fetch_compilers
fetch_objdump
fetch_ido
fetch_mips_objdump
echo "toolchain ready."
