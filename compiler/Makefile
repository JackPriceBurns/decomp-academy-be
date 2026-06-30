# SAM `makefile` builder for the Rust compile service. Builds a static musl
# binary (so it runs on provided.al2023 with no glibc dependency), then stages
# it as `bootstrap` alongside the toolchain — with the native ELF binaries marked
# executable, so they run in place from $LAMBDA_TASK_ROOT with no copy-to-/tmp on
# cold start.
#
# The proprietary MWCC compilers (one mwcceppc.exe per row in compilers.list) and
# the GPL objdump are fetched into vendor/ by fetch-toolchain.sh (not committed);
# only MIT wibo is committed. The fetch is idempotent (no-op on a warm cache).
#
# ONE shared bootstrap + the SAME vendored set ship to every per-version Lambda;
# each Lambda's MWCC_VERSION env var selects which vendored mwcceppc.exe it runs
# (see compile.rs::active_compiler). The pattern rule below therefore builds an
# identical package for every compile function — `build-CompileFunction`,
# `build-CompileFnMwcc233144`, etc. cargo is incremental, so the Rust binary
# compiles once and the rest are a no-op rebuild + copy.

TARGET := x86_64-unknown-linux-musl

.PHONY: fetch-toolchain
fetch-toolchain:
	./scripts/fetch-toolchain.sh

# CARGO_TARGET_DIR (set in CI to a stable path outside SAM's per-build scratch dir)
# lets the compiled-dependency cache survive across runs; falls back to ./target
# for a plain local `sam build`. Only wibo + objdump are native ELF needing +x;
# the mwcceppc.exe / .dll files are read by wibo, never exec'd by the kernel.
build-%: fetch-toolchain
	cargo build --release --target $(TARGET)
	cp $${CARGO_TARGET_DIR:-target}/$(TARGET)/release/bootstrap $(ARTIFACTS_DIR)/bootstrap
	mkdir -p $(ARTIFACTS_DIR)/vendor
	cp -r vendor/. $(ARTIFACTS_DIR)/vendor/
	chmod +x $(ARTIFACTS_DIR)/vendor/toolchain/wibo
	chmod +x $(ARTIFACTS_DIR)/vendor/toolchain/binutils/powerpc-eabi-objdump
