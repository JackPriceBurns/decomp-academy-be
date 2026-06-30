// Auth + user-data HTTP API on the provided.al2023 runtime. API Gateway's JWT
// authorizer validates the token; we just read the claims. Port of src/handlers/api.ts.

use std::sync::Arc;

use aws_sdk_dynamodb::Client;
use lambda_http::{run, service_fn, Body, Error, Request, RequestExt, Response};
use serde::Deserialize;
use serde_json::{json, Value};

use decomp_academy_api::{
    delete_feedback, dynamo, get_all_progress, get_compile_stats, list_feedback, put_feedback,
    record_compile, upsert_progress, FeedbackInput, ProgressInput,
};

// Shared across invocations: the Dynamo client plus a reqwest client reused for
// the Resend call that notifies the site owner of new feedback.
struct AppState {
    db: Client,
    http: reqwest::Client,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let state = Arc::new(AppState { db: dynamo().await, http: reqwest::Client::new() });
    run(service_fn(move |req| {
        let state = state.clone();
        async move { handler(req, &state).await }
    }))
    .await
}

fn resp(status: u16, body: Value) -> Result<Response<Body>, Error> {
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))?)
}

fn ok(body: Value) -> Result<Response<Body>, Error> {
    resp(200, body)
}
fn bad_request(message: &str) -> Result<Response<Body>, Error> {
    resp(400, json!({ "error": { "message": message } }))
}
fn not_found(message: &str) -> Result<Response<Body>, Error> {
    resp(404, json!({ "error": { "message": message } }))
}
fn forbidden(message: &str) -> Result<Response<Body>, Error> {
    resp(403, json!({ "error": { "message": message } }))
}

fn route_key(req: &Request) -> String {
    if let lambda_http::request::RequestContext::ApiGatewayV2(ctx) = req.request_context() {
        ctx.route_key.unwrap_or_default()
    } else {
        String::new()
    }
}

fn claims(req: &Request) -> (Option<String>, Option<String>) {
    if let lambda_http::request::RequestContext::ApiGatewayV2(ctx) = req.request_context() {
        if let Some(jwt) = ctx.authorizer.and_then(|a| a.jwt) {
            return (jwt.claims.get("sub").cloned(), jwt.claims.get("email").cloned());
        }
    }
    (None, None)
}

const ADMIN_GROUP: &str = "admins";

// Admin = membership of the Cognito `admins` group. The HTTP API JWT authorizer
// renders the array `cognito:groups` claim as a string like "[admins editors]".
fn is_admin(req: &Request) -> bool {
    let groups = if let lambda_http::request::RequestContext::ApiGatewayV2(ctx) = req.request_context() {
        ctx.authorizer.and_then(|a| a.jwt).and_then(|jwt| jwt.claims.get("cognito:groups").cloned())
    } else {
        None
    };
    groups
        .map(|g| {
            g.split([',', ' ', '[', ']'])
                .map(|s| s.trim_matches('"').trim())
                .any(|s| s == ADMIN_GROUP)
        })
        .unwrap_or(false)
}

fn path_param<'a>(req: &'a Request, name: &str) -> Option<String> {
    req.path_parameters_ref().and_then(|p| p.first(name).map(|s| s.to_string()))
}

// Lesson IDs are the frontend's stable per-lesson `progressId` — a canonical
// lowercase UUID (8-4-4-4-12). Validating the format keeps malformed/garbage keys
// out of the table: defense in depth for the public /stats route (alongside its
// throttle) and a cheap consistency check on /progress. Rejects anything that
// isn't an exact lowercase-hex UUID.
fn valid_lesson_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        })
}

fn body_bytes(req: &Request) -> Vec<u8> {
    match req.body() {
        Body::Text(s) => s.clone().into_bytes(),
        Body::Binary(b) => b.clone(),
        Body::Empty => Vec::new(),
    }
}

#[derive(Deserialize, Default)]
struct ProgressBody {
    #[serde(rename = "bestPercent")]
    best_percent: Option<f64>,
    code: Option<Value>,
    #[serde(rename = "solvedWithoutHints")]
    solved_without_hints: Option<Value>,
}

#[derive(Deserialize, Default)]
struct StatBody {
    ok: Option<bool>,
}

#[derive(Deserialize, Default)]
struct FeedbackBody {
    #[serde(rename = "lessonId")]
    lesson_id: Option<String>,
    #[serde(rename = "lessonTitle")]
    lesson_title: Option<String>,
    sentiment: Option<String>,
    message: Option<String>,
    email: Option<String>,
    source: Option<String>,
}

async fn handler(req: Request, state: &AppState) -> Result<Response<Body>, Error> {
    let db = &state.db;
    let rk = route_key(&req);

    // Public: anonymous learners give feedback too, so this is handled before the
    // auth guard. It writes the row and best-effort emails the site owner.
    if rk == "POST /feedback" {
        return submit_feedback(&req, state).await;
    }

    // Public: anonymous learners compile too, so this records before the auth guard.
    if rk == "POST /stats/{lessonId}" {
        let Some(lesson_id) = path_param(&req, "lessonId").filter(|id| valid_lesson_id(id)) else {
            return bad_request("lessonId must be a UUID");
        };
        let parsed: StatBody = serde_json::from_slice(&body_bytes(&req)).unwrap_or_default();
        let Some(compiled) = parsed.ok else {
            return bad_request("ok must be a boolean");
        };
        record_compile(db, &lesson_id, compiled).await?;
        return ok(json!({ "recorded": true }));
    }

    let (sub, email) = claims(&req);
    let Some(sub) = sub else {
        return not_found("No identity on request");
    };

    match rk.as_str() {
        "GET /me" => ok(json!({ "sub": sub, "email": email, "isAdmin": is_admin(&req) })),

        "GET /progress" => ok(json!({ "lessons": get_all_progress(db, &sub).await? })),

        "GET /stats" => {
            if !is_admin(&req) {
                return forbidden("Admin access required");
            }
            ok(json!({ "lessons": get_compile_stats(db).await? }))
        }

        "GET /feedback" => {
            if !is_admin(&req) {
                return forbidden("Admin access required");
            }
            ok(json!({ "items": list_feedback(db).await? }))
        }

        "DELETE /feedback/{id}" => {
            if !is_admin(&req) {
                return forbidden("Admin access required");
            }
            let Some(id) = path_param(&req, "id") else {
                return bad_request("Missing feedback id");
            };
            delete_feedback(db, &id).await?;
            ok(json!({ "deleted": true }))
        }

        "PUT /progress/{lessonId}" => {
            let Some(lesson_id) = path_param(&req, "lessonId").filter(|id| valid_lesson_id(id)) else {
                return bad_request("lessonId must be a UUID");
            };
            let parsed: ProgressBody = match serde_json::from_slice(&body_bytes(&req)) {
                Ok(p) => p,
                Err(_) if body_bytes(&req).is_empty() => ProgressBody::default(),
                Err(_) => return bad_request("Body must be JSON"),
            };

            if let Some(p) = parsed.best_percent {
                if !(0.0..=100.0).contains(&p) {
                    return bad_request("bestPercent must be a number between 0 and 100");
                }
            }
            let code = match parsed.code {
                Some(Value::String(s)) => Some(s),
                Some(Value::Null) | None => None,
                Some(_) => return bad_request("code must be a string"),
            };
            let solved_without_hints = match parsed.solved_without_hints {
                Some(Value::Bool(b)) => Some(b),
                Some(Value::Null) | None => None,
                Some(_) => return bad_request("solvedWithoutHints must be a boolean"),
            };

            let updated = upsert_progress(
                db,
                &sub,
                &lesson_id,
                ProgressInput { best_percent: parsed.best_percent, code, solved_without_hints },
            )
            .await?;
            // Flatten { lessonId, ...progress } like the Node handler.
            let mut obj = serde_json::to_value(&updated)?;
            obj["lessonId"] = json!(lesson_id);
            ok(obj)
        }

        other => not_found(&format!("No route for {other}")),
    }
}

// ── Feedback submission (public) ────────────────────────────────────────────

const SENTIMENTS: [&str; 3] = ["good", "confusing", "bug"];
const MAX_MESSAGE: usize = 4000;
const MAX_FIELD: usize = 200;

// Trim, hard-cap the length, and drop to None if empty — so blank strings from
// the form never become stored attributes.
fn trimmed(v: Option<String>, max: usize) -> Option<String> {
    v.map(|s| s.trim().chars().take(max).collect::<String>()).filter(|s| !s.is_empty())
}

async fn submit_feedback(req: &Request, state: &AppState) -> Result<Response<Body>, Error> {
    let parsed: FeedbackBody = match serde_json::from_slice(&body_bytes(req)) {
        Ok(p) => p,
        Err(_) if body_bytes(req).is_empty() => FeedbackBody::default(),
        Err(_) => return bad_request("Body must be JSON"),
    };

    let sentiment = match parsed.sentiment {
        Some(s) if SENTIMENTS.contains(&s.as_str()) => Some(s),
        Some(_) => return bad_request("sentiment must be one of: good, confusing, bug"),
        None => None,
    };
    let message = trimmed(parsed.message, MAX_MESSAGE);
    if sentiment.is_none() && message.is_none() {
        return bad_request("Provide a sentiment or a message");
    }

    let input = FeedbackInput {
        lesson_id: trimmed(parsed.lesson_id, MAX_FIELD),
        lesson_title: trimmed(parsed.lesson_title, MAX_FIELD),
        sentiment,
        message,
        email: trimmed(parsed.email, MAX_FIELD),
        source: trimmed(parsed.source, 40),
    };

    let saved = put_feedback(&state.db, input).await?;

    // Best-effort owner notification: an unconfigured or failing email must never
    // fail the learner's submission — the row is already durably stored.
    notify_owner(&state.http, &saved).await;

    ok(json!({ "recorded": true, "id": saved.id }))
}

fn sentiment_label(s: &Option<String>) -> &'static str {
    match s.as_deref() {
        Some("good") => "👍 Good",
        Some("confusing") => "😕 Confusing",
        Some("bug") => "🐞 Bug",
        _ => "—",
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const FEEDBACK_HTML: &str = include_str!("../../emails/feedback_notification.html");
const FEEDBACK_TEXT: &str = include_str!("../../emails/feedback_notification.txt");

// Email the site owner via Resend (same provider the Cognito email sender uses).
// Silently no-ops when RESEND_API_KEY / FEEDBACK_NOTIFY_EMAIL aren't configured.
async fn notify_owner(http: &reqwest::Client, fb: &decomp_academy_api::FeedbackItem) {
    let api_key = match std::env::var("RESEND_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("RESEND_API_KEY unset — feedback email skipped");
            return;
        }
    };
    let to = match std::env::var("FEEDBACK_NOTIFY_EMAIL") {
        Ok(e) if !e.is_empty() => e,
        _ => {
            eprintln!("FEEDBACK_NOTIFY_EMAIL unset — feedback email skipped");
            return;
        }
    };
    let from_email =
        std::env::var("FROM_EMAIL").unwrap_or_else(|_| "noreply@decomp-academy.dev".into());
    let from_name = std::env::var("FROM_NAME").unwrap_or_else(|_| "Decomp Academy".into());

    let sentiment = sentiment_label(&fb.sentiment);
    let lesson =
        fb.lesson_title.clone().or_else(|| fb.lesson_id.clone()).unwrap_or_else(|| "General".into());
    let email = fb.email.clone().unwrap_or_else(|| "(anonymous)".into());
    let source = fb.source.clone().unwrap_or_else(|| "—".into());
    let message = fb.message.clone().unwrap_or_else(|| "(no message)".into());

    let html = FEEDBACK_HTML
        .replace("__SENTIMENT__", &html_escape(sentiment))
        .replace("__LESSON__", &html_escape(&lesson))
        .replace("__EMAIL__", &html_escape(&email))
        .replace("__SOURCE__", &html_escape(&source))
        .replace("__TIME__", &html_escape(&fb.created_at))
        .replace("__MESSAGE__", &html_escape(&message).replace('\n', "<br>"));
    let text = FEEDBACK_TEXT
        .replace("__SENTIMENT__", sentiment)
        .replace("__LESSON__", &lesson)
        .replace("__EMAIL__", &email)
        .replace("__SOURCE__", &source)
        .replace("__TIME__", &fb.created_at)
        .replace("__MESSAGE__", &message);

    let mut payload = json!({
        "from": format!("{from_name} <{from_email}>"),
        "to": to,
        "subject": format!("New feedback: {sentiment} — {lesson}"),
        "html": html,
        "text": text,
    });
    // Replying to the email reaches the learner directly when they left an address.
    if let Some(ref reply) = fb.email {
        payload["reply_to"] = json!(reply);
    }

    match http.post("https://api.resend.com/emails").bearer_auth(api_key).json(&payload).send().await
    {
        Ok(r) if r.status().is_success() => println!("Feedback email sent ({})", fb.id),
        Ok(r) => eprintln!("Resend feedback email failed ({})", r.status()),
        Err(e) => eprintln!("Resend feedback email error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::valid_lesson_id;

    #[test]
    fn accepts_canonical_lowercase_uuid() {
        // A real progressId from the live table.
        assert!(valid_lesson_id("007fe46b-41ba-5bfa-bf18-1c4e0584dc6e"));
        assert!(valid_lesson_id("0a9d9449-e019-5917-bf35-09afd3bfe00a"));
    }

    #[test]
    fn rejects_non_uuid_keys() {
        assert!(!valid_lesson_id(""));
        assert!(!valid_lesson_id("int64-add")); // the human slug is not the API key
        assert!(!valid_lesson_id("007fe46b41ba5bfabf181c4e0584dc6e")); // no hyphens
        assert!(!valid_lesson_id("007FE46B-41BA-5BFA-BF18-1C4E0584DC6E")); // uppercase
        assert!(!valid_lesson_id("007fe46b-41ba-5bfa-bf18-1c4e0584dc6e ")); // trailing space
        assert!(!valid_lesson_id("g07fe46b-41ba-5bfa-bf18-1c4e0584dc6e")); // non-hex
        assert!(!valid_lesson_id("007fe46b_41ba_5bfa_bf18_1c4e0584dc6e")); // wrong separators
    }
}
