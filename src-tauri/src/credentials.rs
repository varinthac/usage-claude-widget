//! Token discovery chain. Read-only on Claude Code's credentials file;
//! manually-pasted tokens live in Windows Credential Manager only.
//! Token values must never appear in logs, errors, or anything serialized to the frontend.

use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

const KEYRING_SERVICE: &str = "usage-widget-for-claude";
const KEYRING_USER: &str = "manual-token";

#[derive(Deserialize)]
struct CredFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOauth>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOauth {
    access_token: String,
    expires_at: Option<i64>,
    subscription_type: Option<String>,
}

pub struct Token {
    pub value: String,
    pub source: &'static str, // "claude-code" | "manual"
    pub expired: bool,
    pub subscription: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn claude_code_token() -> Option<Token> {
    let path = dirs::home_dir()?.join(".claude").join(".credentials.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: CredFile = serde_json::from_str(&raw).ok()?;
    let oauth = parsed.claude_ai_oauth?;
    if oauth.access_token.trim().is_empty() {
        return None;
    }
    let expired = oauth
        .expires_at
        .map(|v| {
            // Heuristic: values below 10^12 are seconds, otherwise milliseconds.
            let ms = if v < 1_000_000_000_000 { v * 1000 } else { v };
            now_ms() > ms
        })
        .unwrap_or(false);
    Some(Token {
        value: oauth.access_token,
        source: "claude-code",
        expired,
        subscription: oauth.subscription_type,
    })
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
    })
}

/// Claude Code login wins; a manually saved token is the fallback.
pub fn discover() -> Option<Token> {
    claude_code_token().or_else(manual_token)
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
