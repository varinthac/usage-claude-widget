//! Fetches plan usage from the same endpoint Claude Code's /usage command uses.
//! Only percentages / labels / timestamps ever cross the IPC boundary — never the token.

use serde::Serialize;
use serde_json::Value;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Serialize)]
pub struct UsageLimit {
    pub kind: String,
    pub label: String,
    pub percent: f64,
    pub severity: Option<String>,
    pub resets_at: Option<String>,
}

#[derive(Serialize)]
pub struct UsageSnapshot {
    pub plan: Option<String>,
    pub source: String,
    pub limits: Vec<UsageLimit>,
}

#[derive(Serialize)]
pub struct UsageError {
    pub code: &'static str,
    pub message: String,
}

impl UsageError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

fn label_for(kind: &str, scope_name: Option<&str>) -> String {
    match kind {
        "session" => "Current session".into(),
        "weekly_all" => "Weekly · All models".into(),
        "weekly_scoped" => format!("Weekly · {}", scope_name.unwrap_or("Model")),
        other => {
            // Unknown bucket: humanize the kind so new limit types still render.
            let mut label = other.replace('_', " ");
            if let Some(first) = label.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            if let Some(name) = scope_name {
                label = format!("{label} · {name}");
            }
            label
        }
    }
}

fn parse_limits(body: &Value) -> Vec<UsageLimit> {
    let Some(items) = body.get("limits").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let kind = item.get("kind").and_then(Value::as_str)?.to_string();
            let percent = item.get("percent").and_then(Value::as_f64).unwrap_or(0.0);
            let scope_name = item
                .pointer("/scope/model/display_name")
                .and_then(Value::as_str);
            Some(UsageLimit {
                label: label_for(&kind, scope_name),
                kind,
                percent,
                severity: item
                    .get("severity")
                    .and_then(Value::as_str)
                    .map(String::from),
                resets_at: item
                    .get("resets_at")
                    .and_then(Value::as_str)
                    .map(String::from),
            })
        })
        .collect()
}

#[tauri::command]
pub async fn get_usage() -> Result<UsageSnapshot, UsageError> {
    let token = crate::credentials::discover()
        .ok_or_else(|| UsageError::new("no-credentials", "No Claude credentials found."))?;
    if token.expired {
        return Err(UsageError::new(
            "token-expired",
            "The Claude Code token has expired.",
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| UsageError::new("network", "Couldn't build HTTP client."))?;

    let resp = client
        .get(USAGE_URL)
        .bearer_auth(&token.value)
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .await
        .map_err(|e| {
            #[cfg(debug_assertions)]
            eprintln!("[debug] reqwest send error: {e:?}");
            let _ = &e;
            UsageError::new("network", "Request to Anthropic failed.")
        })?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(UsageError::new("unauthorized", "Token was rejected by Anthropic."));
    }
    if !status.is_success() {
        return Err(UsageError::new(
            "api",
            format!("Anthropic API error (HTTP {}).", status.as_u16()),
        ));
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|_| UsageError::new("api", "Unexpected response from Anthropic."))?;

    let limits = parse_limits(&body);
    if limits.is_empty() {
        return Err(UsageError::new(
            "api",
            "Response contained no usage limits — the API shape may have changed.",
        ));
    }

    Ok(UsageSnapshot {
        plan: token.subscription,
        source: token.source.to_string(),
        limits,
    })
}

#[derive(Serialize)]
pub struct CredentialsStatus {
    pub found: bool,
    pub source: Option<&'static str>,
    pub expired: bool,
    pub plan: Option<String>,
}

#[tauri::command]
pub fn credentials_status() -> CredentialsStatus {
    match crate::credentials::discover() {
        Some(tok) => CredentialsStatus {
            found: true,
            source: Some(tok.source),
            expired: tok.expired,
            plan: tok.subscription,
        },
        None => CredentialsStatus {
            found: false,
            source: None,
            expired: false,
            plan: None,
        },
    }
}

#[tauri::command]
pub fn save_manual_token(token: String) -> Result<(), UsageError> {
    crate::credentials::save_manual(&token).map_err(|m| UsageError::new("token-invalid", m))
}

#[tauri::command]
pub fn clear_manual_token() -> Result<(), UsageError> {
    crate::credentials::clear_manual().map_err(|m| UsageError::new("keyring", m))
}
