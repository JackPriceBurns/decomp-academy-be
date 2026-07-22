# Compile toolchain

The compile service runs native binaries from this `vendor/` tree, in place from
`$LAMBDA_TASK_ROOT` (no copy-to-`/tmp` on cold start). They split into **fetched**
(not committed) and **committed**:

| Path | What | Source | Version | In git? |
|---|---|---|---|---|
| `compiler/<family>/<dir>/mwcceppc.exe` + `lmgr*.dll` | Metrowerks CodeWarrior PE32 — one dir per row in [`compilers.list`](../compilers.list) (GC MW 1.0 … Wii MW 1.7) | [files.decomp.dev](https://files.decomp.dev) compilers bundle | `20251118` | **fetched** |
| `toolchain/binutils/powerpc-eabi-objdump` | gekko-patched objdump (static ELF) | [encounter/gc-wii-binutils](https://github.com/encounter/gc-wii-binutils) `linux-x86_64.zip` | `2.42-1` | **fetched** |
| `ido/5.3/*` | SGI IDO 5.3 `cc` driver + compiler passes (cfe/uopt/ugen/as1…), statically recompiled to native x86_64 ELF (N64/MIPS course) | [decompals/ido-static-recomp](https://github.com/decompals/ido-static-recomp) `ido-5.3-recomp-linux.tar.gz` | `v1.2` | **fetched** |
| `toolchain/binutils-mips/mips-ps2-decompals-objdump` | mips objdump (binutils 2.40; v0.4 is the newest release that runs on AL2023's glibc 2.34) | [decompals/binutils-mips-ps2-decompals](https://github.com/decompals/binutils-mips-ps2-decompals) `linux-x86-64.tar.gz` | `v0.4` | **fetched** |
| `toolchain/binutils-mips/lib/libzstd.so.1` | the mips objdump's one shared-lib dep (BSD); vendored because the Lambda runtime may not ship zstd — the service sets `LD_LIBRARY_PATH` for the objdump call | Debian bullseye `libzstd1_1.4.8+dfsg-2.1_amd64.deb` via content-addressed [snapshot.debian.org](https://snapshot.debian.org) | `1.4.8` | **fetched** |
| `toolchain/wibo` | Win32 loader (Linux x86_64, static ELF) | [decompals/wibo](https://github.com/decompals/wibo) `static-release64` CI artifact | `main` @ `cd743b0` (incl. [#130](https://github.com/decompals/wibo/pull/130)) | **committed** |

Every supported MWCC version is vendored at `compiler/<family>/<dir>/` (mirroring
the bundle, e.g. `compiler/GC/2.0/`, `compiler/Wii/1.7/`). The same package ships
to every per-version Lambda; each Lambda's `MWCC_VERSION` env var selects which
`mwcceppc.exe` it runs. `mwcceppc.exe` is verified against the SHA-1 in
`compilers.list`; the license DLL (`lmgr326b.dll` for GC, `lmgr8c.dll` for
Wii/3.0a) is one shared, byte-identical file pinned in `fetch-toolchain.sh`.

## Why the split

- **MWCC is proprietary.** Metrowerks CodeWarrior must not be redistributed, so
  it is never committed — `scripts/fetch-toolchain.sh` pulls it from the decomp
  compilers bundle at build time. Bring-your-own-compiler: nothing copyrighted
  lives in this repo.
- **objdump is GPL.** Fetching it from its upstream release (rather than
  committing it) keeps this repo free of any GPL binary and its
  accompanying-source obligation. It's a stable tagged release, so fetching is
  reliable.
- **wibo is MIT and committed** (license in `toolchain/wibo.LICENSE`). The
  seccomp-fallback fix ([#130](https://github.com/decompals/wibo/pull/130)) that
  lets stock wibo run on Lambda is **not in any release** — `1.1.0` is six commits
  behind the merge (`cd743b0`). Its only build is the `static-release64` CI
  artifact off `main`, and CI artifacts expire after ~90 days, so fetching it
  would make builds break unpredictably. Committing the 7 MB MIT binary is the
  robust choice.
  > **TODO:** once wibo ships a release `> 1.1.0` that includes #130, move wibo
  > into `fetch-toolchain.sh` (release `wibo-x86_64` asset) and stop committing it.

## Fetching

`scripts/fetch-toolchain.sh` downloads the two fetched binaries into this tree
and verifies each against a pinned SHA-1 (so a changed upstream is caught and the
compiler can't silently drift). It is **idempotent** — a binary already present
with the right hash is left alone — and runs automatically as a prerequisite of
the `build-CompileFunction` Make target (so `sam build` just works). To populate
the tree by hand:

```sh
compiler/scripts/fetch-toolchain.sh   # or: make -C compiler fetch-toolchain
```

To bump a version, edit the `*_TAG` and `SHA_*` constants at the top of
`scripts/fetch-toolchain.sh` together.

> All three binaries are **x86_64 only**. wibo executes the 32-bit `mwcceppc.exe`
> directly, which cannot be emulated under Docker on Apple Silicon (Rosetta and
> QEMU both fail on its segment handling) — it runs only on real x86_64. The
> fetch always pulls the x86_64 Linux / PE32 builds regardless of host OS.
