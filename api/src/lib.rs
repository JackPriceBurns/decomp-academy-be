// Shared data layer for the user-data API: DynamoDB single-table access for
// per-lesson progress and compile stats. Port of the old src/lib/{progress,stats}.ts.

use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use serde::Serialize;
use std::collections::HashMap;

pub type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub fn table_name() -> String {
    std::env::var("TABLE_NAME").expect("TABLE_NAME must be set")
}

pub async fn dynamo() -> Client {
    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    Client::new(&cfg)
}

fn pk(sub: &str) -> String {
    format!("USER#{sub}")
}
fn sk(lesson_id: &str) -> String {
    format!("PROGRESS#{lesson_id}")
}

fn as_n(item: &HashMap<String, AttributeValue>, key: &str) -> Option<f64> {
    item.get(key).and_then(|v| v.as_n().ok()).and_then(|s| s.parse().ok())
}
fn as_s(item: &HashMap<String, AttributeValue>, key: &str) -> Option<String> {
    item.get(key).and_then(|v| v.as_s().ok()).cloned()
}
fn as_bool(item: &HashMap<String, AttributeValue>, key: &str) -> Option<bool> {
    item.get(key).and_then(|v| v.as_bool().ok()).copied()
}

#[derive(Serialize)]
pub struct LessonProgress {
    #[serde(rename = "bestPercent")]
    pub best_percent: f64,
    pub completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(rename = "solvedWithoutHints", skip_serializing_if = "Option::is_none")]
    pub solved_without_hints: Option<bool>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

fn to_progress(item: &HashMap<String, AttributeValue>) -> LessonProgress {
    LessonProgress {
        best_percent: as_n(item, "bestPercent").unwrap_or(0.0),
        completed: as_bool(item, "completed").unwrap_or(false),
        code: as_s(item, "code"),
        solved_without_hints: as_bool(item, "solvedWithoutHints"),
        updated_at: as_s(item, "updatedAt").unwrap_or_default(),
    }
}

pub async fn get_all_progress(
    db: &Client,
    sub: &str,
) -> Result<HashMap<String, LessonProgress>, DynError> {
    let mut lessons = HashMap::new();
    let mut start_key: Option<HashMap<String, AttributeValue>> = None;
    loop {
        let res = db
            .query()
            .table_name(table_name())
            .key_condition_expression("PK = :pk AND begins_with(SK, :sk)")
            .expression_attribute_values(":pk", AttributeValue::S(pk(sub)))
            .expression_attribute_values(":sk", AttributeValue::S("PROGRESS#".into()))
            .set_exclusive_start_key(start_key.clone())
            .send()
            .await?;
        for item in res.items() {
            if let Some(id) = as_s(item, "lessonId") {
                lessons.insert(id, to_progress(item));
            }
        }
        match res.last_evaluated_key() {
            Some(k) if !k.is_empty() => start_key = Some(k.clone()),
            _ => break,
        }
    }
    Ok(lessons)
}

pub struct ProgressInput {
    pub best_percent: Option<f64>,
    pub code: Option<String>,
    pub solved_without_hints: Option<bool>,
}

// bestPercent only ever moves up; code is overwritten with the latest saved.
// Earning the "no hints" badge on any device sticks; only meaningful once solved.
pub async fn upsert_progress(
    db: &Client,
    sub: &str,
    lesson_id: &str,
    input: ProgressInput,
) -> Result<LessonProgress, DynError> {
    let existing = db
        .get_item()
        .table_name(table_name())
        .key("PK", AttributeValue::S(pk(sub)))
        .key("SK", AttributeValue::S(sk(lesson_id)))
        .send()
        .await?;
    let prev = existing.item;

    let prev_best = prev.as_ref().and_then(|p| as_n(p, "bestPercent")).unwrap_or(0.0);
    let best_percent = prev_best.max(input.best_percent.unwrap_or(0.0));
    let code = input.code.or_else(|| prev.as_ref().and_then(|p| as_s(p, "code")));
    let completed = best_percent >= 100.0;

    let prev_no_hints = prev.as_ref().and_then(|p| as_bool(p, "solvedWithoutHints"));
    let solved_without_hints = if completed {
        Some(prev_no_hints == Some(true) || input.solved_without_hints == Some(true))
    } else {
        None
    };

    let updated_at = now_iso8601();

    let mut item: HashMap<String, AttributeValue> = HashMap::new();
    item.insert("PK".into(), AttributeValue::S(pk(sub)));
    item.insert("SK".into(), AttributeValue::S(sk(lesson_id)));
    item.insert("lessonId".into(), AttributeValue::S(lesson_id.to_string()));
    item.insert("bestPercent".into(), AttributeValue::N(fmt_num(best_percent)));
    item.insert("completed".into(), AttributeValue::Bool(completed));
    if let Some(ref c) = code {
        item.insert("code".into(), AttributeValue::S(c.clone()));
    }
    if let Some(b) = solved_without_hints {
        item.insert("solvedWithoutHints".into(), AttributeValue::Bool(b));
    }
    item.insert("updatedAt".into(), AttributeValue::S(updated_at.clone()));

    db.put_item()
        .table_name(table_name())
        .set_item(Some(item))
        .send()
        .await?;

    Ok(LessonProgress { best_percent, completed, code, solved_without_hints, updated_at })
}

// ── Compile stats (one counter item per lesson) ─────────────────────────────

const STATS_SK: &str = "COMPILE_STATS";

#[derive(Serialize)]
pub struct CompileStat {
    #[serde(rename = "lessonId")]
    pub lesson_id: String,
    pub attempts: f64,
    pub failures: f64,
    #[serde(rename = "failRate")]
    pub fail_rate: f64,
    #[serde(rename = "lastAt", skip_serializing_if = "Option::is_none")]
    pub last_at: Option<String>,
}

pub async fn record_compile(db: &Client, lesson_id: &str, compiled: bool) -> Result<(), DynError> {
    db.update_item()
        .table_name(table_name())
        .key("PK", AttributeValue::S(format!("LESSON#{lesson_id}")))
        .key("SK", AttributeValue::S(STATS_SK.into()))
        .update_expression("SET lessonId = :lid, lastAt = :now ADD attempts :one, failures :fail")
        .expression_attribute_values(":lid", AttributeValue::S(lesson_id.to_string()))
        .expression_attribute_values(":now", AttributeValue::S(now_iso8601()))
        .expression_attribute_values(":one", AttributeValue::N("1".into()))
        .expression_attribute_values(":fail", AttributeValue::N(if compiled { "0" } else { "1" }.into()))
        .send()
        .await?;
    Ok(())
}

// Low cardinality (a few hundred lessons), so a filtered Scan is fine — no GSI.
pub async fn get_compile_stats(db: &Client) -> Result<Vec<CompileStat>, DynError> {
    let mut stats = Vec::new();
    let mut start_key: Option<HashMap<String, AttributeValue>> = None;
    loop {
        let res = db
            .scan()
            .table_name(table_name())
            .filter_expression("SK = :sk")
            .expression_attribute_values(":sk", AttributeValue::S(STATS_SK.into()))
            .set_exclusive_start_key(start_key.clone())
            .send()
            .await?;
        for item in res.items() {
            let attempts = as_n(item, "attempts").unwrap_or(0.0);
            let failures = as_n(item, "failures").unwrap_or(0.0);
            stats.push(CompileStat {
                lesson_id: as_s(item, "lessonId").unwrap_or_default(),
                attempts,
                failures,
                fail_rate: if attempts > 0.0 { failures / attempts } else { 0.0 },
                last_at: as_s(item, "lastAt"),
            });
        }
        match res.last_evaluated_key() {
            Some(k) if !k.is_empty() => start_key = Some(k.clone()),
            _ => break,
        }
    }
    stats.sort_by(|a, b| b.failures.partial_cmp(&a.failures).unwrap_or(std::cmp::Ordering::Equal));
    Ok(stats)
}

// ── Feedback (one partition, listed newest-first) ───────────────────────────
// Every feedback row lives under a single partition so the admin view lists it
// with one Query; volume is low. SK = "{createdAt}#{suffix}" sorts by time, and
// a sub-millisecond suffix keeps it unique without pulling in a random/uuid dep.

const FEEDBACK_PK: &str = "FEEDBACK";

pub struct FeedbackInput {
    pub lesson_id: Option<String>,
    pub lesson_title: Option<String>,
    pub sentiment: Option<String>,
    pub message: Option<String>,
    pub email: Option<String>,
    pub source: Option<String>,
}

#[derive(Serialize)]
pub struct FeedbackItem {
    // The SK, also used as the opaque id the admin DELETE addresses.
    pub id: String,
    #[serde(rename = "lessonId", skip_serializing_if = "Option::is_none")]
    pub lesson_id: Option<String>,
    #[serde(rename = "lessonTitle", skip_serializing_if = "Option::is_none")]
    pub lesson_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentiment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    // Set once an admin has emailed the learner back. `replied_at` drives the
    // "Replied" state in the admin view; `reply_message` is the latest reply sent.
    #[serde(rename = "repliedAt", skip_serializing_if = "Option::is_none")]
    pub replied_at: Option<String>,
    #[serde(rename = "replyMessage", skip_serializing_if = "Option::is_none")]
    pub reply_message: Option<String>,
}

pub async fn put_feedback(db: &Client, input: FeedbackInput) -> Result<FeedbackItem, DynError> {
    let created_at = now_iso8601();
    let id = format!("{created_at}#{}", unique_suffix());

    let mut item: HashMap<String, AttributeValue> = HashMap::new();
    item.insert("PK".into(), AttributeValue::S(FEEDBACK_PK.into()));
    item.insert("SK".into(), AttributeValue::S(id.clone()));
    item.insert("createdAt".into(), AttributeValue::S(created_at.clone()));
    for (key, val) in [
        ("lessonId", &input.lesson_id),
        ("lessonTitle", &input.lesson_title),
        ("sentiment", &input.sentiment),
        ("message", &input.message),
        ("email", &input.email),
        ("source", &input.source),
    ] {
        if let Some(v) = val {
            item.insert(key.into(), AttributeValue::S(v.clone()));
        }
    }

    db.put_item().table_name(table_name()).set_item(Some(item)).send().await?;

    Ok(FeedbackItem {
        id,
        lesson_id: input.lesson_id,
        lesson_title: input.lesson_title,
        sentiment: input.sentiment,
        message: input.message,
        email: input.email,
        source: input.source,
        created_at,
        replied_at: None,
        reply_message: None,
    })
}

pub async fn list_feedback(db: &Client) -> Result<Vec<FeedbackItem>, DynError> {
    let mut items = Vec::new();
    let mut start_key: Option<HashMap<String, AttributeValue>> = None;
    loop {
        let res = db
            .query()
            .table_name(table_name())
            .key_condition_expression("PK = :pk")
            .expression_attribute_values(":pk", AttributeValue::S(FEEDBACK_PK.into()))
            .scan_index_forward(false) // newest first
            .set_exclusive_start_key(start_key.clone())
            .send()
            .await?;
        for item in res.items() {
            items.push(FeedbackItem {
                id: as_s(item, "SK").unwrap_or_default(),
                lesson_id: as_s(item, "lessonId"),
                lesson_title: as_s(item, "lessonTitle"),
                sentiment: as_s(item, "sentiment"),
                message: as_s(item, "message"),
                email: as_s(item, "email"),
                source: as_s(item, "source"),
                created_at: as_s(item, "createdAt").unwrap_or_default(),
                replied_at: as_s(item, "repliedAt"),
                reply_message: as_s(item, "replyMessage"),
            });
        }
        match res.last_evaluated_key() {
            Some(k) if !k.is_empty() => start_key = Some(k.clone()),
            _ => break,
        }
    }
    Ok(items)
}

// Fetch a single feedback row by its id (the SK). Returns None when the id
// doesn't exist — the reply route maps that to a 404.
pub async fn get_feedback(db: &Client, id: &str) -> Result<Option<FeedbackItem>, DynError> {
    let res = db
        .get_item()
        .table_name(table_name())
        .key("PK", AttributeValue::S(FEEDBACK_PK.into()))
        .key("SK", AttributeValue::S(id.to_string()))
        .send()
        .await?;
    let Some(item) = res.item else { return Ok(None) };
    Ok(Some(FeedbackItem {
        id: as_s(&item, "SK").unwrap_or_else(|| id.to_string()),
        lesson_id: as_s(&item, "lessonId"),
        lesson_title: as_s(&item, "lessonTitle"),
        sentiment: as_s(&item, "sentiment"),
        message: as_s(&item, "message"),
        email: as_s(&item, "email"),
        source: as_s(&item, "source"),
        created_at: as_s(&item, "createdAt").unwrap_or_default(),
        replied_at: as_s(&item, "repliedAt"),
        reply_message: as_s(&item, "replyMessage"),
    }))
}

// Record that the owner has emailed the learner back: stamp the reply time and
// store the latest reply text. Only written after the email actually sends, so a
// row is never marked replied for a message the learner didn't receive.
pub async fn mark_feedback_replied(
    db: &Client,
    id: &str,
    reply_message: &str,
) -> Result<String, DynError> {
    let replied_at = now_iso8601();
    db.update_item()
        .table_name(table_name())
        .key("PK", AttributeValue::S(FEEDBACK_PK.into()))
        .key("SK", AttributeValue::S(id.to_string()))
        .update_expression("SET repliedAt = :at, replyMessage = :msg")
        .expression_attribute_values(":at", AttributeValue::S(replied_at.clone()))
        .expression_attribute_values(":msg", AttributeValue::S(reply_message.to_string()))
        .send()
        .await?;
    Ok(replied_at)
}

pub async fn delete_feedback(db: &Client, id: &str) -> Result<(), DynError> {
    db.delete_item()
        .table_name(table_name())
        .key("PK", AttributeValue::S(FEEDBACK_PK.into()))
        .key("SK", AttributeValue::S(id.to_string()))
        .send()
        .await?;
    Ok(())
}

// Sub-millisecond component of the current instant, zero-padded so it sorts
// lexically. createdAt (ms precision) dominates the SK; this just breaks ties
// between two submissions landing in the same millisecond.
fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{nanos:09}")
}

// Whole integers serialize without a trailing ".0" (matches the JS number form).
fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

// Minimal ISO-8601 UTC timestamp (no chrono dep), e.g. 2026-06-26T19:33:01.000Z.
fn now_iso8601() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let millis = dur.subsec_millis();
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, m, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

// Howard Hinnant's days-from-civil, inverted. days = count since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
