# SAM `makefile` builder for the two Rust user-data Lambdas, cross-compiled to
# arm64 (Graviton) with cargo-lambda + zig. zig's `cc` cross-compiles aws-lc-sys's
# C and targets an older glibc, so the binary runs on provided.al2023.
# CARGO_TARGET_DIR (set in CI to a stable path outside SAM's per-build scratch dir)
# lets the compiled-dependency cache survive across runs; falls back to ./target
# for a plain local `sam build`.
build-ApiFunction:
	cargo lambda build --release --arm64 --bin api
	cp $${CARGO_TARGET_DIR:-target}/lambda/api/bootstrap $(ARTIFACTS_DIR)/bootstrap

build-CognitoEmailSenderFunction:
	cargo lambda build --release --arm64 --bin email-sender
	cp $${CARGO_TARGET_DIR:-target}/lambda/email-sender/bootstrap $(ARTIFACTS_DIR)/bootstrap
