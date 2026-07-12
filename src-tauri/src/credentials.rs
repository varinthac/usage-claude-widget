//! Token discovery chain, plus OAuth refresh for Claude Code logins.
//!
//! The only write this module ever performs on Claude Code's credentials
//! file is storing a freshly-refreshed token pair — the same refresh flow
//! and file shape Claude Code itself uses, so both apps stay in sync.
//! Manually-pasted tokens live in Windows Credential Manager only.
//! Token values must never appear in logs, errors, or anything serialized
//! to the frontend.

use serde::Deserialize;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const KEYRING_SERVICE: &str = "usage-widget-for-claude";
const KEYRING_USER: &str = "manual-token";

const OAUTH_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
/// Claude Code's public OAuth client id (the same one its own login flow uses).
const CLAUDE_CODE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// Treat a token as expired slightly early so an in-flight request never
/// races the actual expiry.
const EXPIRY_MARGIN_MS: i64 = 120_000;

#[derive(Deserialize)]
struct CredFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOauth>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOauth {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    subscription_type: Option<String>,
}

pub struct Token {
    pub value: String,
    pub source: &'static str, // "claude-code" | "manual"
    pub expired: bool,
    pub subscription: Option<String>,
    pub refresh: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn cred_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join(".credentials.json"))
}

#[allow(unused_variables, unused_assignments)]
fn claude_code_token() -> Option<Token> {
    let Some(path) = cred_path() else {
        #[cfg(debug_assertions)]
        eprintln!("[debug] claude_code_token: couldn't resolve home dir");
        return None;
    };

    // Claude Code can rewrite this file (e.g. token refresh) at almost the same
    // moment we read it; retry a couple of times before giving up so a transient
    // read/parse race doesn't get misreported as "not logged in".
    let mut last_err: Option<String> = None;
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            Err(e) => {
                last_err = Some(format!("read failed: {e}"));
                continue;
            }
        };
        let parsed: CredFile = match serde_json::from_str(&raw) {
            Ok(p) => p,
            Err(e) => {
                last_err = Some(format!("parse failed: {e}"));
                continue;
            }
        };
        let Some(oauth) = parsed.claude_ai_oauth else {
            last_err = Some("missing claudeAiOauth key".to_string());
            continue;
        };
        if oauth.access_token.trim().is_empty() {
            last_err = Some("accessToken is empty".to_string());
            continue;
        }
        let expired = oauth
            .expires_at
            .map(|v| {
                // Heuristic: values below 10^12 are seconds, otherwise milliseconds.
                let ms = if v < 1_000_000_000_000 { v * 1000 } else { v };
                now_ms() > ms - EXPIRY_MARGIN_MS
            })
            .unwrap_or(false);
        return Some(Token {
            value: oauth.access_token,
            source: "claude-code",
            expired,
            subscription: oauth.subscription_type,
            refresh: oauth.refresh_token,
        });
    }

    #[cfg(debug_assertions)]
    if let Some(e) = last_err {
        eprintln!("[debug] claude_code_token: giving up — {e}");
    }
    None
}

fn keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
}

fn manual_token() -> Option<Token> {
    let value = keyring_entry().ok()?.get_password().ok()?;
    if value.trim().is_empty() {
        return None;
    }
    Some(Token {
        value,
        source: "manual",
        expired: false,
        subscription: None,
        refresh: None,
    })
}

/// Claude Code login wins; a manually saved token is the fallback.
pub fn discover() -> Option<Token> {
    claude_code_token().or_else(manual_token)
}

/// Refresh an expired Claude Code token using its refresh token — the same
/// OAuth flow Claude Code itself runs — and store the result back in
/// `.credentials.json` so Claude Code keeps working with the new pair.
/// Returns generic error strings only; never token material.
pub async fn refresh_claude_code() -> Result<Token, String> {
    // Re-read first: Claude Code (or another widget instance) may have
    // refreshed in the meantime, in which case just use that.
    let tok = claude_code_token().ok_or("no Claude Code credentials")?;
    if !tok.expired {
        return Ok(tok);
    }
    let refresh_token = tok.refresh.clone().ok_or("no refresh token stored")?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| "couldn't build HTTP client")?;

    let resp = client
        .post(OAUTH_TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLAUDE_CODE_CLIENT_ID,
        }))
        .send()
        .await
        .map_err(|_| "refresh request failed (network)")?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("refresh rejected (HTTP {})", status.as_u16()));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| "unexpected refresh response")?;
    let access = body
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .ok_or("refresh response missing access_token")?
        .to_string();
    let new_refresh = body
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .map(String::from);
    let expires_in = body
        .get("expires_in")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(8 * 3600);
    let expires_at = now_ms() + expires_in * 1000;

    write_back_refreshed(&access, new_refresh.as_deref(), expires_at)?;

    Ok(Token {
        value: access,
        source: "claude-code",
        expired: false,
        subscription: tok.subscription,
        refresh: new_refresh.or(Some(refresh_token)),
    })
}

/// Update only the token fields in `.credentials.json`, preserving every
/// other field verbatim, via an atomic temp-file + rename.
fn write_back_refreshed(
    access: &str,
    new_refresh: Option<&str>,
    expires_at: i64,
) -> Result<(), String> {
    let path = cred_path().ok_or("couldn't resolve home dir")?;
    let raw = std::fs::read_to_string(&path).map_err(|_| "couldn't re-read credentials file")?;
    let mut v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| "couldn't parse credentials file")?;
    let oauth = v
        .get_mut("claudeAiOauth")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("credentials file missing claudeAiOauth")?;
    oauth.insert("accessToken".into(), access.into());
    if let Some(r) = new_refresh {
        oauth.insert("refreshToken".into(), r.into());
    }
    oauth.insert("expiresAt".into(), expires_at.into());

    let tmp = path.with_file_name(".credentials.json.usage-widget-tmp");
    let serialized = serde_json::to_string(&v).map_err(|_| "couldn't serialize credentials")?;
    std::fs::write(&tmp, serialized).map_err(|_| "couldn't write temp credentials file")?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("couldn't replace credentials file: {e}")
    })?;
    Ok(())
}

pub fn save_manual(token: &str) -> Result<(), String> {
    let token = token.trim();
    if !token.starts_with("sk-ant-") || token.len() < 20 {
        return Err("That doesn't look like a Claude OAuth token (should start with sk-ant-).".into());
    }
    keyring_entry()
        .and_then(|e| e.set_password(token))
        .map_err(|_| "Couldn't write to Windows Credential Manager.".to_string())
}

pub fn clear_manual() -> Result<(), String> {
    match keyring_entry().and_then(|e| e.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Couldn't remove the saved token.".into()),
    }
}
