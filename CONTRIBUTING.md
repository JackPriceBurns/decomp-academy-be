# Contributing to the Decomp Academy API

Thanks for your interest in improving the backend! Contributions are very
welcome — bug fixes, compile-service improvements, new API features, docs, and
infra hardening.

> **Adding lessons?** Lesson content lives in the frontend repo,
> [`decomp-academy-fe`](https://github.com/JackPriceBurns/decomp-academy-fe), not
> here. This repo is the compile service and the auth/user-data API.

## Prerequisites

Everything is Rust on AWS Lambda, built with AWS SAM. You'll need:

- **Rust** (stable) with the Lambda targets:
  ```sh
  rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-gnu
  ```
- **[AWS SAM CLI](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html)** — `sam build` / `sam validate`.
- For the arm64 Lambdas (api, email-sender): **[cargo-lambda](https://www.cargo-lambda.info/)** + **[zig](https://ziglang.org/)** (cross-linking).
- For the compile service: **musl-tools** (`musl-gcc`), plus `curl` + `unzip` for the toolchain fetch.

A read of the [README](README.md) — especially the architecture diagram and the
project layout — is the fastest way to get oriented.

### The compile toolchain

The compile service runs the real Metrowerks CodeWarrior GC/2.0 compiler. It is
**proprietary and not redistributed in this repo** — it's fetched at build time
(SHA-pinned) by [`compiler/scripts/fetch-toolchain.sh`](compiler/scripts/fetch-toolchain.sh),
which `sam build` runs automatically. You can also run it directly:

```sh
compiler/scripts/fetch-toolchain.sh    # or: make -C compiler fetch-toolchain
```

See [`compiler/vendor/README.md`](compiler/vendor/README.md) for what is fetched
vs. committed and why.

## Build & test

```sh
# Per crate — the fast inner loop:
cargo build --release            # in api/ or compiler/
cargo test                       # compiler/ has unit tests for the flag/diag logic

# Whole stack:
sam build                        # cross-compiles all three Lambdas via their Makefiles
sam validate --lint              # CloudFormation/SAM template check
```

Please run `cargo fmt` and `cargo clippy` before opening a PR, and match the style
of the surrounding code — comments explain the non-obvious *why*, not the *what*.

## Pull requests

The standard GitHub fork-and-PR flow:

1. **Fork** and clone:
   ```sh
   git clone https://github.com/<your-username>/decomp-academy-api
   cd decomp-academy-api
   ```
2. **Branch**:
   ```sh
   git checkout -b fix-compile-timeout
   ```
3. **Make your change**, with a focused commit and a clear, imperative message
   (e.g. `Bound /compile memory on warm containers`). Keep unrelated changes out
   of the same PR.
4. **Verify** it builds and tests pass (`cargo test`, `sam validate --lint`); if
   you touched the compile path, confirm a real compile still works.
5. **Open a PR** against `main` describing what changed and why.

Deploys run automatically from `main` via GitHub Actions, so PRs don't deploy —
they just need to build and pass review. Infra changes (`template.yaml`) get
extra scrutiny: call out anything that affects the no-egress compile sandbox,
IAM, or the retained resources (Cognito user pool, DynamoDB table, KMS key).

## Secrets & config

Never commit account-specific values (account ids, ARNs, zone ids, emails, API
keys). Deploy-time values are supplied as GitHub Actions secrets and SAM
parameters — see the secrets table in the [README](README.md#deploy). Public
defaults (domain, from-address, CORS origins) live in `template.yaml`.

## Reporting security issues

Please **don't** open a public issue for a security vulnerability — especially
anything touching the compile sandbox (it runs untrusted code). Report it
privately to the maintainer instead.

## License

By contributing, you agree that your contributions are licensed under the
project's **[AGPL-3.0](LICENSE)** license.
