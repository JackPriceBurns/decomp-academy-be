// Cognito Custom Email Sender trigger on the provided.al2023 runtime. Cognito
// hands over the OTP as an AWS Encryption SDK message (base64), which we decrypt
// with a KMS keyring (commitment policy REQUIRE_ENCRYPT_ALLOW_DECRYPT, since
// Cognito's messages are non-committing), then stamp into a pre-rendered email
// and send via Resend. Port of src/handlers/cognitoCustomEmailSender.ts.

use std::collections::HashMap;
use std::sync::Arc;

use aws_config::SdkConfig;
use base64::Engine;
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use serde::Deserialize;
use serde_json::json;

use aws_esdk::client as esdk_client;
use aws_esdk::material_providers::client as mpl_client;
use aws_esdk::material_providers::types::material_providers_config::MaterialProvidersConfig;
use aws_esdk::material_providers::types::EsdkCommitmentPolicy;
use aws_esdk::types::aws_encryption_sdk_config::AwsEncryptionSdkConfig;

const CODE_PLACEHOLDER: &str = "__OTP_CODE__";

struct Template {
    html: &'static str,
    text: &'static str,
    subject: &'static str,
    label: &'static str,
}

// Map a Cognito trigger to the email to render. Returns None for triggers we
// don't handle (the same set the Node handler covered).
fn plan(trigger_source: &str) -> Option<Template> {
    match trigger_source {
        "CustomEmailSender_SignUp" | "CustomEmailSender_ResendCode" => Some(Template {
            html: include_str!("../../emails/verification_signup.html"),
            text: include_str!("../../emails/verification_signup.txt"),
            subject: "Verify your Decomp Academy account",
            label: "signup",
        }),
        "CustomEmailSender_UpdateUserAttribute" | "CustomEmailSender_VerifyUserAttribute" => {
            Some(Template {
                html: include_str!("../../emails/verification_verify_email.html"),
                text: include_str!("../../emails/verification_verify_email.txt"),
                subject: "Confirm your Decomp Academy email",
                label: "verify-email",
            })
        }
        "CustomEmailSender_ForgotPassword" => Some(Template {
            html: include_str!("../../emails/password_reset.html"),
            text: include_str!("../../emails/password_reset.txt"),
            subject: "Reset your Decomp Academy password",
            label: "password-reset",
        }),
        _ => None,
    }
}

#[derive(Deserialize)]
struct EmailEvent {
    #[serde(rename = "triggerSource")]
    trigger_source: String,
    request: EmailRequest,
}

#[derive(Deserialize)]
struct EmailRequest {
    code: Option<String>,
    #[serde(rename = "userAttributes")]
    user_attributes: HashMap<String, String>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let sdk = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let http = reqwest::Client::new();
    let shared = Arc::new((sdk, http));
    run(service_fn(move |event| {
        let shared = shared.clone();
        async move { handler(event, &shared.0, &shared.1).await }
    }))
    .await
}

async fn decrypt_code(sdk: &SdkConfig, encrypted: &str) -> Result<String, Error> {
    let key_arn = std::env::var("KMS_KEY_ARN").map_err(|_| "KMS_KEY_ARN must be set")?;

    let esdk_config = AwsEncryptionSdkConfig::builder()
        .commitment_policy(EsdkCommitmentPolicy::RequireEncryptAllowDecrypt)
        .build()
        .map_err(|e| format!("esdk config: {e:?}"))?;
    let esdk = esdk_client::Client::from_conf(esdk_config).map_err(|e| format!("esdk client: {e:?}"))?;

    let mpl = mpl_client::Client::from_conf(
        MaterialProvidersConfig::builder().build().map_err(|e| format!("mpl config: {e:?}"))?,
    )
    .map_err(|e| format!("mpl client: {e:?}"))?;

    let keyring = mpl
        .create_aws_kms_keyring()
        .kms_key_id(key_arn)
        .kms_client(aws_sdk_kms::Client::new(sdk))
        .send()
        .await
        .map_err(|e| format!("create keyring: {e:?}"))?;

    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(encrypted)
        .map_err(|e| format!("base64 decode: {e}"))?;

    let out = esdk
        .decrypt()
        .ciphertext(aws_smithy_types::Blob::new(ciphertext))
        .keyring(keyring)
        .send()
        .await
        .map_err(|e| format!("decrypt: {e:?}"))?;

    let plaintext = out.plaintext.ok_or("decrypt returned no plaintext")?;
    Ok(String::from_utf8(plaintext.into_inner())?)
}

async fn send_email(
    http: &reqwest::Client,
    to: &str,
    subject: &str,
    html: &str,
    text: &str,
) -> Result<(), Error> {
    let from_email = std::env::var("FROM_EMAIL").map_err(|_| "FROM_EMAIL must be set")?;
    let from_name = std::env::var("FROM_NAME").unwrap_or_else(|_| "Decomp Academy".into());
    let api_key = std::env::var("RESEND_API_KEY").map_err(|_| "RESEND_API_KEY is not configured")?;

    let res = http
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&json!({
            "from": format!("{from_name} <{from_email}>"),
            "to": to,
            "subject": subject,
            "html": html,
            "text": text,
        }))
        .send()
        .await?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Resend send failed ({status}): {body}").into());
    }
    Ok(())
}

async fn handler(
    event: LambdaEvent<EmailEvent>,
    sdk: &SdkConfig,
    http: &reqwest::Client,
) -> Result<(), Error> {
    let event = event.payload;

    let Some(template) = plan(&event.trigger_source) else {
        println!("Unhandled Cognito custom email trigger: {}", event.trigger_source);
        return Ok(());
    };

    let (Some(encrypted), Some(email)) =
        (event.request.code.as_deref(), event.request.user_attributes.get("email"))
    else {
        eprintln!("Missing code or email on Cognito event ({})", event.trigger_source);
        return Ok(());
    };

    // Require Resend to be configured: a missing/empty key means we can't deliver
    // the code, so error out (failing the Cognito operation) rather than silently
    // dropping it — and never decrypt or log the plaintext OTP.
    if std::env::var("RESEND_API_KEY").map(|k| k.is_empty()).unwrap_or(true) {
        return Err("RESEND_API_KEY is not configured".into());
    }

    let code = decrypt_code(sdk, encrypted).await?;
    let html = template.html.replace(CODE_PLACEHOLDER, &code);
    let text = template.text.replace(CODE_PLACEHOLDER, &code);
    send_email(http, email, template.subject, &html, &text).await?;
    println!("Verification email sent ({}, {})", event.trigger_source, template.label);
    Ok(())
}
