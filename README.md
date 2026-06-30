# Decomp Academy API

**The serverless backend behind [Decomp Academy](https://decomp-academy.dev) — it runs the real 2001 Metrowerks CodeWarrior GC/2.0 compiler inside AWS Lambda to grade learners' C byte-for-byte, and stores their progress.**

[![Live: decomp-academy.dev](https://img.shields.io/badge/live-decomp--academy.dev-6d28d9)](https://decomp-academy.dev)
[![License: AGPL v3](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
[![Runtime: Rust on Lambda](https://img.shields.io/badge/runtime-Rust%20%2F%20provided.al2023-orange.svg)](#tech)

This is the API half of Decomp Academy; the Next.js frontend lives in
[`decomp-academy-fe`](https://github.com/JackPriceBurns/decomp-academy-fe). When a
learner writes C to match a PowerPC function, the browser sends it here, the
**actual MWCC GC/2.0 compiler** (`mwcceppc.exe`) compiles it to a GameCube object,
and the browser diffs that against the target instruction-for-instruction. The
same stack also handles accounts and saves per-lesson progress.

Everything is **one [AWS SAM](https://aws.amazon.com/serverless/sam/) stack of
Rust Lambdas** — no servers, no containers, no Node or JVM cold-start. It splits
into two halves that share nothing but the template and the HTTP API in front of
them.

## Architecture

```
                        https://api.decomp-academy.dev   ·   one HTTP API
                                         │
          ┌──────────────────────────────┴─────────────────────────────────┐
          │  public compile routes                    JWT-protected routes │
          │  /health  /target  /check               /me  /progress  /stats │
          ▼                                                                ▼
 ┌───────────────────────┐                              ┌────────────────────────┐
 │    CompileFunction    │  x86_64 · static-musl        │      ApiFunction       │ arm64
 │  wibo → MWCC GC/2.0   │  stateless                   │  Cognito JWT authorizer│
 │  → gekko objdump      │                              │  → DynamoDB (progress) │
 └───────────────────────┘                              └────────────────────────┘

  browser ──SRP, directly──▶ Cognito ──encrypted OTP──▶ EmailSenderFunction  arm64
                                                        (aws-esdk decrypt → Resend)
```

- **Compile service** — stateless; turns learner C into a PowerPC object. Public,
  no auth. [Jump ↓](#compile-service)
- **Auth & user data** — Cognito accounts, a JWT-protected API, and DynamoDB for
  per-lesson progress and compile stats. [Jump ↓](#auth--user-data)

<a name="tech"></a>Every function is **Rust on `provided.al2023`** (static
binaries). The compile service is x86_64 / static-musl (so `wibo` can exec the
32-bit `mwcceppc.exe`); the API and email-sender are arm64, cross-compiled with
`cargo-lambda` + `zig`. The browser does the byte-accurate assembly diff with
[objdiff](https://github.com/encounter/objdiff)-wasm, so the API only has to
produce the object and a symbol list.

## Compile service

A Rust Lambda (`CompileFunction`) on `provided.al2023`, x86_64, sharing the HTTP
API and custom domain. It runs the **real Metrowerks CodeWarrior GC/2.0** compiler
under [`wibo`](https://github.com/decompals/wibo) (a tiny Win32 loader), then
parses the result with a gekko-patched `objdump`. The routes are public.

| Method | Path | Body | Returns |
|---|---|---|---|
| `GET` | `/health` | — | `{ ok, version }` |
| `POST` | `/target` | `{ solution, symbol, context?, extraFlags? }` | `{ ok, instructions, objBase64 }` |
| `POST` | `/check` | `{ code, symbol, context?, extraFlags? }` | `{ ok, objBase64, symbols }` |

### Toolchain

The toolchain is pinned under `compiler/vendor/` and runs **in place from
`$LAMBDA_TASK_ROOT`** — no copy-to-`/tmp` on cold start. It splits by what's legal
and practical to commit:

- **Fetched at build time** (SHA-pinned, never committed) by
  [`compiler/scripts/fetch-toolchain.sh`](compiler/scripts/fetch-toolchain.sh):
  the proprietary **MWCC GC/2.0** compiler (`mwcceppc.exe` + `lmgr326b.dll`, from
  the [decomp.dev](https://files.decomp.dev) compilers bundle) and the GPL
  **`powerpc-eabi-objdump`** (from [`gc-wii-binutils`](https://github.com/encounter/gc-wii-binutils)).
- **Committed**: only MIT-licensed **`wibo`**.

See [`compiler/vendor/README.md`](compiler/vendor/README.md) for the full
rationale. `sam build` runs the fetch automatically.

> **Lambda + wibo footnote.** Lambda's seccomp filter traps the 32-bit
> `int 0x80` `set_thread_area` with `SIGSYS`. wibo probes for this at startup and
> falls back to `modify_ldt` on its own
> ([decompals/wibo#130](https://github.com/decompals/wibo/pull/130)), so stock
> wibo runs unmodified on Lambda with no env gate.

## Auth & user data

### How auth works

The frontend talks to **Cognito directly** (SRP via `amazon-cognito-identity-js` /
Amplify) for register, confirm, login, and refresh — auth is **not** proxied
through Lambda. It then calls this API with the Cognito **ID token** in the
`Authorization` header, and API Gateway's built-in JWT authorizer validates it
(no custom authorizer code).

> Send the **ID token**, not the access token — the access token omits the
> `email` claim that `/me` returns.

### Endpoints

All JWT-protected, except where noted.

| Method | Path | Body | Returns |
|---|---|---|---|
| `GET` | `/me` | — | `{ sub, email }` |
| `GET` | `/progress` | — | `{ lessons: { [lessonId]: { bestPercent, completed, code, updatedAt } } }` |
| `PUT` | `/progress/{lessonId}` | `{ bestPercent?, code? }` | the updated lesson record |
| `GET` | `/stats` | — | `{ lessons: [{ lessonId, attempts, failures, failRate, lastAt }] }` |
| `POST` | `/stats/{lessonId}` | `{ ok }` | `{ recorded: true }` — **public** (no JWT): anonymous learners compile too |

`bestPercent` only ever moves up; `code` is the learner's last saved source. This
mirrors what the frontend keeps in `localStorage` (solved %, partial progress,
last code per lesson) so it can sync server-side.

### Data model — DynamoDB single table

`decomp-academy-api-data`, `PK` / `SK`, `PAY_PER_REQUEST`, with PITR and deletion
protection on:

- **Progress** — `PK=USER#<cognitoSub>`, `SK=PROGRESS#<lessonId>`.
- **Compile stats** — `PK=LESSON#<lessonId>`, `SK=COMPILE_STATS`, with `attempts`
  / `failures` counters bumped atomically by `POST /stats/{lessonId}`. The
  frontend reports each compile outcome after `/check`; the compile service itself
  has no concept of lessons.

### Transactional email

Verification and password-reset codes go out through a Cognito **Custom Email
Sender** Lambda (`EmailSenderFunction`). Cognito encrypts the OTP with a KMS key
and hands it to the Lambda, which decrypts it with the **AWS Encryption SDK**
(`aws-esdk`) and sends a pre-rendered email via [Resend](https://resend.com).

> The raw `kms:Decrypt` API does **not** work here: Cognito's `request.code` is an
> Encryption-SDK message, so it must be decrypted through the keyring with
> `EsdkCommitmentPolicy::RequireEncryptAllowDecrypt` (see
> `api/src/bin/email_sender.rs`).

Email bodies are authored as [react-email](https://react.email) templates in
`src/emails/` and pre-rendered to static HTML/text by `npm run emails:export`
(into `api/emails/`, with an `__OTP_CODE__` placeholder). The Rust Lambda
`include_str!`s those and substitutes the code at runtime — **React is never in
the runtime path.**

Transactional email **requires Resend to be configured**: if `RESEND_API_KEY` is
unset, the email-sender trigger errors (failing the Cognito sign-up / reset)
rather than delivering — it never logs the plaintext code. Set it up before
relying on auth:

1. Add `decomp-academy.dev` as a domain in Resend and add the SPF/DKIM records to
   its Route53 zone (the `HOSTED_ZONE_ID` secret).
2. Set the `RESEND_API_KEY` (`re_…`) GitHub Actions secret (passed to the
   `ResendApiKey` stack parameter).
3. Confirm `FromEmail` (default `noreply@decomp-academy.dev`) is on that domain.

## Building & running

All three Lambdas are Rust, built by SAM via per-function `Makefile`s. No Node in
the build or runtime path.

```sh
cargo build --release    # per crate (api/, compiler/) — fast inner loop
sam build                # invokes each Makefile builder; fetches the toolchain
sam deploy               # or just push to main (see below)
```

The `src/emails/` react-email templates are dev-only authoring tooling:

```sh
npm ci
npm run email            # live preview at :3001
npm run emails:export    # re-render to api/emails/*.{html,txt}
```

### Deploy

Deploys run in GitHub Actions ([`.github/workflows/deploy.yml`](.github/workflows/deploy.yml))
on every push to `main`: install Rust + the build toolchains (musl-tools for the
compile service; `cargo-lambda` + `zig` for the arm64 Lambdas) → `sam build` →
`sam deploy`.

Account-specific values are kept out of the repo and supplied as **GitHub Actions
secrets** (Settings → Secrets and variables → Actions):

| Secret | Value | Used for |
|---|---|---|
| `AWS_DEPLOY_ROLE_ARN` | `arn:aws:iam::<account-id>:role/<role-name>` | OIDC role the workflow assumes |
| `CERTIFICATE_ARN` | `arn:aws:acm:<region>:<account-id>:certificate/<cert-id>` | `CertificateArn` parameter |
| `HOSTED_ZONE_ID` | `<route53-zone-id>` | `HostedZoneId` parameter |
| `FEEDBACK_NOTIFY_EMAIL` | `you@example.com` (optional) | `FeedbackNotifyEmail` parameter |
| `RESEND_API_KEY` | `re_…` (optional) | `ResendApiKey` parameter |

Public defaults stay in `template.yaml`: `DomainName`, `FromEmail`,
`AllowedOrigins`.

### Frontend wiring

Point [`decomp-academy-fe`](https://github.com/JackPriceBurns/decomp-academy-fe)
at the deployed stack — the values are in the stack Outputs (`UserPoolId`,
`UserPoolClientId`, `Region`, `ApiUrl`):

```sh
NEXT_PUBLIC_AWS_REGION=eu-west-1
NEXT_PUBLIC_COGNITO_USER_POOL_ID=<UserPoolId>
NEXT_PUBLIC_COGNITO_CLIENT_ID=<UserPoolClientId>
NEXT_PUBLIC_API_URL=https://api.decomp-academy.dev
```

### DNS + TLS

`api.decomp-academy.dev` is served by the HTTP API's regional custom domain. The
ACM certificate is created and DNS-validated **manually** (regional); its ARN
embeds the account id, so it's passed in at deploy time via the `CERTIFICATE_ARN`
secret rather than committed. SAM manages the rest: the API Gateway custom domain,
the base-path mapping, and the Route53 alias record.

## Project layout

```
template.yaml                 SAM/CFN: 3 Rust Lambdas + Cognito / HTTP API / DynamoDB
samconfig.toml                stack/region defaults (decomp-academy-api, eu-west-1)
.github/workflows/deploy.yml  GHA: Rust toolchains → sam build → sam deploy
api/                          auth + user-data Lambdas (Rust, arm64)
  src/lib.rs                    DynamoDB progress + compile-stats data layer
  src/bin/api.rs                /me, /progress, /stats router (HTTP API, JWT)
  src/bin/email_sender.rs       Cognito OTP decrypt (aws-esdk) + Resend send
  emails/                       pre-rendered HTML/text (generated; embedded in binary)
compiler/                     the compile service (Rust, x86_64)
  src/main.rs                   HTTP API router (/health, /target, /check)
  src/compile.rs                wibo + MWCC compile, objdump parse, diagnostics
  scripts/fetch-toolchain.sh    fetch MWCC + objdump at build time (SHA-pinned)
  vendor/                       committed wibo (MIT); MWCC + objdump fetched here
src/emails/ + scripts/        react-email design source + export script (dev only)
```

## Related projects

- **[decomp-academy-fe](https://github.com/JackPriceBurns/decomp-academy-fe)** —
  the Next.js frontend and the lesson curriculum.
- **[wibo](https://github.com/decompals/wibo)** · **[gc-wii-binutils](https://github.com/encounter/gc-wii-binutils)**
  · **[objdiff](https://github.com/encounter/objdiff)** — the toolchain that makes
  byte-matching grading possible.
- The wider decompilation community: [decomp.me](https://decomp.me) ·
  [decomp.dev](https://decomp.dev) · [the decomp wiki](https://wiki.decomp.dev).

## Contributing

Contributions are welcome — bug fixes, compile-service improvements, and API
features. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, build/test, and the
PR flow. (Lesson content goes in the [frontend repo](https://github.com/JackPriceBurns/decomp-academy-fe).)

## License

Decomp Academy is free software licensed under the **GNU Affero General Public
License v3.0** ([AGPL-3.0](LICENSE)). You're free to use, study, modify, and
redistribute it — but any derivative work, **including a modified version run as a
network service**, must also be released under the AGPL-3.0 with its complete
source. See [LICENSE](LICENSE) for the full text.
