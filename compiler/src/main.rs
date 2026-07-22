// AWS Lambda (HTTP API, payload format 2.0) entry point for the compile
// service — MWCC (PowerPC, under wibo) on /compile/mwcc/{version}/* plus the
// legacy flat routes, and IDO 5.3 (MIPS, native ELF) on /compile/ido/5.3/* —
// on the `provided.al2023` custom runtime. The vendored toolchains ship under
// vendor/ inside the deployment package and run in place from $LAMBDA_TASK_ROOT
// (the Makefile marks the binaries executable at build time) — no copying to
// /tmp on cold start. wibo self-detects Lambda's seccomp-blocked
// set_thread_area and falls back to modify_ldt (decompals/wibo#130).

mod compile;

use lambda_http::{run, service_fn, Body, Error, Request, Response};
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_BODY: usize = 100_000;
const MAX_CODE: usize = 20_000;

#[derive(Deserialize, Default)]
struct CompileBody {
    code: Option<String>,
    solution: Option<String>,
    symbol: Option<String>,
    context: Option<String>,
    /// "c" (default) or "cpp" — selects -lang and the source file extension.
    /// MWCC-only: the IDO routes are C-only and ignore this field.
    language: Option<String>,
    /// Optimization preset, validated per toolchain (MWCC: compile::ALLOWED_OPT,
    /// default "O4,p"; IDO: compile::IDO_ALLOWED_OPT, default "O2,g3").
    opt: Option<String>,
    /// Disable the peephole / instruction-scheduling passes (default: enabled).
    /// MWCC-only: ignored on the IDO routes (IDO has no such toggles).
    peephole: Option<bool>,
    schedule: Option<bool>,
    /// Compiler version id; only "GC/2.0" is supported today.
    compiler: Option<String>,
    #[serde(rename = "withTypes")]
    with_types: Option<bool>,
    // NOTE: there is deliberately no `extraFlags` here. Forwarding caller-supplied
    // compiler flags is an arbitrary-file-read vector — e.g. `-include /proc/self/environ`
    // reads any file the sandbox can see and leaks it back through the error
    // diagnostics, bypassing the #include source guard. All codegen options are
    // expressed via the validated `language`/`opt`/`peephole`/`schedule` fields
    // above; any unknown JSON fields (incl. a stray "extraFlags") are ignored.
}

/// Shared codegen knobs derived from a request body, with that toolchain's
/// defaults (opt validation is per-toolchain; the rest only matter for MWCC).
fn opt_fields(toolchain: compile::Toolchain, body: &CompileBody) -> (compile::Language, String, bool, bool) {
    (
        compile::Language::parse(body.language.as_deref()),
        toolchain.validate_opt(body.opt.as_deref()),
        body.peephole.unwrap_or(true),
        body.schedule.unwrap_or(true),
    )
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    match std::env::args().nth(1).as_deref() {
        Some("selftest") => return selftest(compile::Toolchain::Mwcc).await,
        Some("selftest-ido") => return selftest(compile::Toolchain::Ido53).await,
        Some("leaktest") => {
            let n = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(200usize);
            return leaktest(n).await;
        }
        _ => {}
    }
    run(service_fn(handler)).await
}

/// Local leak probe: loops compile() N times in ONE process and samples this
/// process's RSS + any lingering toolchain child processes. A monotonic RSS climb
/// here (macOS) means a real leak in compile() (allocations/fds); a flat RSS would
/// point at musl allocator retention on Lambda instead. Run with the local
/// toolchain env: `WIBO=… MWCC=… OBJDUMP=… cargo run --release -- leaktest 200`.
async fn leaktest(iters: usize) -> Result<(), Error> {
    let pid = std::process::id().to_string();
    let rss_kb = || {
        std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    // Count stray toolchain processes (a child-reaping leak shows here, not in RSS).
    let strays = || {
        std::process::Command::new("sh")
            .arg("-c")
            .arg("ps axo comm | grep -cE 'wibo|mwcceppc|powerpc-eabi-objdump' || true")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    // Cycle through the distinct code paths so an input-specific leak (the
    // failing-compile path, the context-struct path, C++/mangling, a bigger
    // function) can't hide behind a trivial fixed request.
    let big = "int poly(int a,int b,int c,int d,int e){int x=a*b+c;int y=d-e;int z=x*y;\
        for(int i=0;i<e;i++){z+=a*i-b;}return z*x+y;}";
    let variants: [(&str, Option<&str>, &str, compile::Language); 4] = [
        ("int add(int a,int b){return a+b;}", None, "add", compile::Language::C),
        ("int oops(){ return nope; }", None, "oops", compile::Language::C), // fails to compile
        ("int getX(Pt* p){ return p->x; }", Some("typedef struct { int x, y; } Pt;"), "getX", compile::Language::C),
        (big, None, "poly", compile::Language::Cpp),
    ];
    println!("iter 0: rss(KB)={} strays={}", rss_kb(), strays());
    for i in 0..iters {
        let (code, ctx, sym, lang) = variants[i % variants.len()];
        let _ = compile::compile(compile::Request {
            code,
            context: ctx,
            symbol: sym,
            language: lang,
            opt: compile::DEFAULT_OPT.to_string(),
            peephole: true,
            schedule: true,
            extra_flags: Vec::new(),
            with_types: true,
            toolchain: compile::Toolchain::Mwcc,
        })
        .await;
        if (i + 1) % 20 == 0 {
            println!("iter {}: rss(KB)={} strays={}", i + 1, rss_kb(), strays());
        }
    }
    Ok(())
}

async fn handler(event: Request) -> Result<Response<Body>, Error> {
    let method = event.method().as_str().to_string();
    let path = {
        let p = event.uri().path().trim_end_matches('/');
        if p.is_empty() { "/".to_string() } else { p.to_string() }
    };
    let body: Vec<u8> = match event.body() {
        Body::Text(s) => s.clone().into_bytes(),
        Body::Binary(b) => b.clone(),
        Body::Empty => Vec::new(),
    };
    Ok(route(&method, &path, &body).await)
}

fn json_resp(status: u16, value: Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .expect("response builds")
}

/// Parse the request JSON. `None` means the body is too large (-> 413); a parse
/// error yields an empty struct, matching the original handler's lenient parsing.
fn parse_body(raw: &[u8]) -> Option<CompileBody> {
    if raw.len() > MAX_BODY {
        return None;
    }
    Some(serde_json::from_slice(raw).unwrap_or_default())
}

// Three route families reach the same three handlers:
//   * the original flat routes — `POST /target|/check|/compile`, `GET /health`
//   * the versioned MWCC structure — `/compile/mwcc/{version}/{action}` — added
//     so the frontend can pick a compiler/version per lesson. The flat routes
//     stay live and byte-identical so deploying this can't break the frontend.
//   * the IDO family — `/compile/ido/5.3/{action}` — same handlers running the
//     IDO 5.3 toolchain (N64/MIPS course).
// On the versioned routes the URL is authoritative for compiler/version; the
// body `compiler` field is ignored (the path already names the compiler).
async fn route(method: &str, path: &str, raw: &[u8]) -> Response<Body> {
    use RouteSel::{Flat, Ido53, MwccVersion};
    let segs: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    match (method, segs.as_slice()) {
        ("GET", ["health"]) | ("GET", []) => handle_health(compile::Toolchain::Mwcc).await,
        ("GET", ["compile", "mwcc", version, "health"]) => match validate_path_version(version) {
            Ok(()) => handle_health(compile::Toolchain::Mwcc).await,
            Err(r) => r,
        },
        ("GET", ["compile", "ido", "5.3", "health"]) => handle_health(compile::Toolchain::Ido53).await,

        ("POST", ["target"]) => with_body(raw, Flat, handle_target).await,
        ("POST", ["check"]) => with_body(raw, Flat, handle_check).await,
        ("POST", ["compile"]) => with_body(raw, Flat, handle_compile).await,

        ("POST", ["compile", "mwcc", version, "target"]) => with_body(raw, MwccVersion(version), handle_target).await,
        ("POST", ["compile", "mwcc", version, "check"]) => with_body(raw, MwccVersion(version), handle_check).await,
        ("POST", ["compile", "mwcc", version, "compile"]) => with_body(raw, MwccVersion(version), handle_compile).await,

        // The IDO version is a literal: any other /compile/ido/... path 404s
        // below (a new IDO build would be a new literal + Lambda, like MWCC).
        ("POST", ["compile", "ido", "5.3", "target"]) => with_body(raw, Ido53, handle_target).await,
        ("POST", ["compile", "ido", "5.3", "check"]) => with_body(raw, Ido53, handle_check).await,
        ("POST", ["compile", "ido", "5.3", "compile"]) => with_body(raw, Ido53, handle_compile).await,

        _ => json_resp(404, json!({ "ok": false, "error": format!("No route for {method} {path}.") })),
    }
}

/// Which route family matched, and therefore how to validate the request and
/// which toolchain the handler runs.
enum RouteSel<'a> {
    /// Legacy flat routes: validate the body's `compiler` field, run MWCC.
    Flat,
    /// /compile/mwcc/{version}/*: validate the path token, run MWCC.
    MwccVersion(&'a str),
    /// /compile/ido/5.3/*: the literal path is the validation, run IDO.
    Ido53,
}

impl RouteSel<'_> {
    fn toolchain(&self) -> compile::Toolchain {
        match self {
            RouteSel::Ido53 => compile::Toolchain::Ido53,
            _ => compile::Toolchain::Mwcc,
        }
    }
}

/// Parse + validate a request body, then run `handler` with the route's toolchain.
async fn with_body<F, Fut>(raw: &[u8], sel: RouteSel<'_>, handler: F) -> Response<Body>
where
    F: FnOnce(compile::Toolchain, CompileBody) -> Fut,
    Fut: std::future::Future<Output = Response<Body>>,
{
    let Some(body) = parse_body(raw) else {
        return json_resp(413, json!({ "ok": false, "error": "Body too large or unreadable." }));
    };
    let validation = match &sel {
        RouteSel::MwccVersion(v) => compile::validate_version(v),
        RouteSel::Flat => compile::validate_compiler(body.compiler.as_deref()),
        RouteSel::Ido53 => Ok(()),
    };
    if let Err(e) = validation {
        return json_resp(200, json!({ "ok": false, "error": e }));
    }
    handler(sel.toolchain(), body).await
}

/// `GET /compile/mwcc/{version}/health` has no body to thread through `with_body`.
fn validate_path_version(v: &str) -> Result<(), Response<Body>> {
    compile::validate_version(v).map_err(|e| json_resp(200, json!({ "ok": false, "error": e })))
}

async fn handle_health(toolchain: compile::Toolchain) -> Response<Body> {
    json_resp(200, json!({ "ok": true, "version": compile::compiler_version(toolchain).await }))
}

async fn handle_target(toolchain: compile::Toolchain, body: CompileBody) -> Response<Body> {
    let solution = body.solution.as_deref();
    let symbol = body.symbol.as_deref().filter(|s| !s.is_empty());
    let (Some(solution), Some(symbol)) = (solution, symbol) else {
        return json_resp(400, json!({ "ok": false, "error": "target requires { solution, symbol }." }));
    };
    let (language, opt, peephole, schedule) = opt_fields(toolchain, &body);
    let out = compile::compile(compile::Request {
        code: solution,
        context: body.context.as_deref(),
        symbol,
        language,
        opt,
        peephole,
        schedule,
        extra_flags: Vec::new(), // caller flags are never trusted (see CompileBody)
        with_types: body.with_types.unwrap_or(true),
        toolchain,
    })
    .await;
    if out.ok {
        json_resp(200, json!({ "ok": true, "instructions": out.instructions, "objBase64": out.obj_base64 }))
    } else {
        json_resp(200, json!({ "ok": false, "error": out.diagnostics }))
    }
}

async fn handle_check(toolchain: compile::Toolchain, body: CompileBody) -> Response<Body> {
    let code = body.code.as_deref();
    let symbol = body.symbol.as_deref().filter(|s| !s.is_empty());
    let (Some(code), Some(symbol)) = (code, symbol) else {
        return json_resp(400, json!({ "ok": false, "error": "check requires { code, symbol }." }));
    };
    if code.len() > MAX_CODE {
        return json_resp(413, json!({ "ok": false, "error": "Code too long." }));
    }
    let (language, opt, peephole, schedule) = opt_fields(toolchain, &body);
    let out = compile::compile(compile::Request {
        code,
        context: body.context.as_deref(),
        symbol,
        language,
        opt,
        peephole,
        schedule,
        extra_flags: Vec::new(), // caller flags are never trusted (see CompileBody)
        with_types: body.with_types.unwrap_or(true),
        toolchain,
    })
    .await;
    if out.ok {
        json_resp(200, json!({ "ok": true, "objBase64": out.obj_base64, "symbols": out.symbols }))
    } else {
        json_resp(200, json!({ "ok": false, "compileError": out.diagnostics, "symbols": out.symbols }))
    }
}

async fn handle_compile(toolchain: compile::Toolchain, body: CompileBody) -> Response<Body> {
    let Some(code) = body.code.as_deref() else {
        return json_resp(400, json!({ "ok": false, "error": "compile requires { code }." }));
    };
    if code.len() > MAX_CODE {
        return json_resp(413, json!({ "ok": false, "error": "Code too long." }));
    }
    let (language, opt, peephole, schedule) = opt_fields(toolchain, &body);
    // Free-form playground compile: no lesson, no fixed symbol. An empty
    // symbol tells compile() to return the object + every function symbol;
    // the browser disassembles + picks a function with objdiff.
    let out = compile::compile(compile::Request {
        code,
        context: body.context.as_deref(),
        symbol: "",
        language,
        opt,
        peephole,
        schedule,
        extra_flags: Vec::new(), // caller flags are never trusted (see CompileBody)
        with_types: body.with_types.unwrap_or(true),
        toolchain,
    })
    .await;
    if out.ok {
        json_resp(200, json!({ "ok": true, "objBase64": out.obj_base64, "symbols": out.symbols }))
    } else {
        json_resp(200, json!({ "ok": false, "compileError": out.diagnostics, "symbols": out.symbols }))
    }
}

/// Local validation: `bootstrap selftest` (MWCC, via WIBO/MWCC/OBJDUMP env
/// overrides) or `bootstrap selftest-ido` (IDO 5.3, via IDO_CC/MIPS_OBJDUMP)
/// compiles a trivial function and prints the outcome as JSON.
async fn selftest(toolchain: compile::Toolchain) -> Result<(), Error> {
    let out = compile::compile(compile::Request {
        code: "int add(int a, int b){ return a + b; }",
        context: None,
        symbol: "add",
        language: compile::Language::C,
        opt: toolchain.validate_opt(None),
        peephole: true,
        schedule: true,
        extra_flags: Vec::new(),
        with_types: true,
        toolchain,
    })
    .await;
    println!(
        "{}",
        json!({
            "ok": out.ok,
            "symbols": out.symbols,
            "instructionCount": out.instructions.len(),
            "diagnostics": out.diagnostics,
            "objBytes": out.obj_base64.as_deref().map(|s| s.len()).unwrap_or(0),
        })
    );
    if !out.ok {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod compat_tests {
    use super::*;

    // A request body exactly as the *currently deployed* frontend sends it — no
    // language/opt/peephole/schedule/compiler fields. The new backend must treat
    // it as today's defaults (C, O4,p, peephole+schedule on, GC/2.0), so deploying
    // the backend ahead of the frontend can't change existing C codegen.
    #[test]
    fn legacy_request_body_resolves_to_todays_defaults() {
        let legacy = r#"{"code":"int add(int a,int b){return a+b;}","symbol":"add","context":null,"extraFlags":[],"withTypes":true}"#;
        let body: CompileBody = serde_json::from_str(legacy).expect("legacy body parses");

        assert!(compile::validate_compiler(body.compiler.as_deref()).is_ok());
        let (language, opt, peephole, schedule) = opt_fields(compile::Toolchain::Mwcc, &body);
        assert!(matches!(language, compile::Language::C));
        assert_eq!(opt, "O4,p");
        assert!(peephole);
        assert!(schedule);
    }

    // The same body on the IDO route resolves to IDO defaults — and an MWCC
    // preset in `opt` falls back rather than leaking across toolchains.
    #[test]
    fn ido_opt_fields_use_ido_defaults() {
        let body: CompileBody =
            serde_json::from_str(r#"{"code":"x","symbol":"f"}"#).expect("parses");
        let (_, opt, _, _) = opt_fields(compile::Toolchain::Ido53, &body);
        assert_eq!(opt, "O2,g3");
        let body: CompileBody =
            serde_json::from_str(r#"{"code":"x","symbol":"f","opt":"O4,p"}"#).expect("parses");
        let (_, opt, _, _) = opt_fields(compile::Toolchain::Ido53, &body);
        assert_eq!(opt, "O2,g3");
        let body: CompileBody =
            serde_json::from_str(r#"{"code":"x","symbol":"f","opt":"O1"}"#).expect("parses");
        let (_, opt, _, _) = opt_fields(compile::Toolchain::Ido53, &body);
        assert_eq!(opt, "O1");
    }

    // Unknown future fields must not break parsing either (serde ignores them).
    #[test]
    fn unknown_fields_are_ignored() {
        let body: CompileBody =
            serde_json::from_str(r#"{"code":"x","somethingNew":123}"#).expect("ignores unknown");
        assert_eq!(body.code.as_deref(), Some("x"));
    }

    fn body_string(resp: Response<Body>) -> String {
        match resp.into_body() {
            Body::Text(s) => s,
            Body::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
            Body::Empty => String::new(),
        }
    }

    // The validation/routing layer can be exercised without the toolchain: every
    // assertion below returns BEFORE compile() is reached (bad version, missing
    // fields, or unknown path), so these run in plain CI with no vendored binaries.

    // Both route families must reach the SAME handler. An empty body has no
    // code/symbol, so the check handler 400s — proving the versioned path lands
    // there just like the flat route, without invoking the compiler.
    #[tokio::test]
    async fn flat_and_versioned_check_reach_the_same_handler() {
        assert_eq!(route("POST", "/check", b"{}").await.status(), 400);
        // 247_92 = GC/2.0 (a real token from compilers.list).
        assert_eq!(route("POST", "/compile/mwcc/247_92/check", b"{}").await.status(), 400);
    }

    #[tokio::test]
    async fn versioned_target_and_compile_route_through() {
        assert_eq!(route("POST", "/compile/mwcc/247_92/target", b"{}").await.status(), 400);
        // A non-default version routes the same way (here Wii/1.7).
        assert_eq!(route("POST", "/compile/mwcc/43_213/check", b"{}").await.status(), 400);
        // /compile/mwcc/{v}/compile needs only { code }; empty body 400s pre-compile.
        assert_eq!(route("POST", "/compile/mwcc/247_92/compile", b"{}").await.status(), 400);
    }

    #[tokio::test]
    async fn versioned_route_rejects_unknown_version() {
        let resp = route("POST", "/compile/mwcc/9.9/check", b"{}").await;
        assert_eq!(resp.status(), 200);
        assert!(body_string(resp).contains("Unsupported mwcc version"));
    }

    // Unknown compiler segments fall through to 404 (a different compiler would
    // be a different route family/Lambda, like ido below).
    #[tokio::test]
    async fn unknown_compiler_and_paths_404() {
        assert_eq!(route("POST", "/compile/other/1.0/check", b"{}").await.status(), 404);
        assert_eq!(route("GET", "/nope", b"").await.status(), 404);
    }

    // The IDO family reaches the same three handlers (empty body 400s before
    // any compiler is invoked, proving the route landed), its version segment
    // is a literal (anything else 404s), and its health needs no toolchain
    // probe (safe to assert the body in plain CI).
    #[tokio::test]
    async fn ido_routes_reach_handlers() {
        assert_eq!(route("POST", "/compile/ido/5.3/check", b"{}").await.status(), 400);
        assert_eq!(route("POST", "/compile/ido/5.3/target", b"{}").await.status(), 400);
        assert_eq!(route("POST", "/compile/ido/5.3/compile", b"{}").await.status(), 400);
    }

    #[tokio::test]
    async fn ido_version_segment_is_literal() {
        assert_eq!(route("POST", "/compile/ido/9.9/check", b"{}").await.status(), 404);
        assert_eq!(route("POST", "/compile/ido/5.3.1/check", b"{}").await.status(), 404);
        assert_eq!(route("GET", "/compile/ido/9.9/health", b"").await.status(), 404);
    }

    #[tokio::test]
    async fn ido_health_reports_static_label() {
        let resp = route("GET", "/compile/ido/5.3/health", b"").await;
        assert_eq!(resp.status(), 200);
        let body = body_string(resp);
        assert!(body.contains("\"ok\":true"), "got: {body}");
        assert!(body.contains("IDO 5.3"), "got: {body}");
    }

    // The body `compiler` field is ignored on the ido routes (URL authoritative),
    // like the versioned mwcc routes: it still 400s on missing fields rather
    // than erroring on the compiler name.
    #[tokio::test]
    async fn ido_route_ignores_body_compiler_field() {
        let resp = route("POST", "/compile/ido/5.3/check", br#"{"compiler":"GC/2.0"}"#).await;
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn versioned_health_validates_version() {
        // Unknown version on the versioned health route is rejected (200 + error)
        // without probing the compiler banner.
        let resp = route("GET", "/compile/mwcc/9.9/health", b"").await;
        assert_eq!(resp.status(), 200);
        assert!(body_string(resp).contains("Unsupported mwcc version"));
    }
}
