//! Local persistence helpers for cache, history, and site preferences.
//!
//! These helpers intentionally use plain JSON files under the user's cache
//! directory. The issue asks for local, opt-in workflow state; a file-backed
//! store keeps that state inspectable and avoids adding a database dependency
//! before the CLI has enough history volume to justify it.

use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::KagiError;

const CACHE_DIR_ENV: &str = "KAGI_CACHE_DIR";
const DEFAULT_CACHE_SUBDIR: &str = ".cache/kagi-cli";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheEnvelope {
    pub created_at: u64,
    pub ttl_seconds: u64,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub command: String,
    pub query: Option<String>,
    pub result_count: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SitePreferences {
    pub domains: BTreeMap<String, SitePreferenceMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum SitePreferenceMode {
    Block,
    Lower,
    Normal,
    Higher,
    Pin,
}

impl SitePreferenceMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Lower => "lower",
            Self::Normal => "normal",
            Self::Higher => "higher",
            Self::Pin => "pin",
        }
    }
}

pub fn now_unix_seconds() -> Result<u64, KagiError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| KagiError::Config(format!("system clock is before UNIX epoch: {error}")))
}

pub fn cache_root() -> PathBuf {
    env::var(CACHE_DIR_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_cache_root)
}

pub fn cache_key(parts: &[&str]) -> String {
    let mut hasher = DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
        0xff_u8.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

pub fn cache_get(key: &str) -> Result<Option<Value>, KagiError> {
    let path = cache_response_path(key);
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path).map_err(|error| {
        KagiError::Config(format!(
            "failed to read cache entry {}: {error}",
            path.display()
        ))
    })?;
    let envelope: CacheEnvelope = serde_json::from_str(&raw).map_err(|error| {
        KagiError::Parse(format!(
            "failed to parse cache entry {}: {error}",
            path.display()
        ))
    })?;
    let now = now_unix_seconds()?;

    if now.saturating_sub(envelope.created_at) > envelope.ttl_seconds {
        let _ = fs::remove_file(path);
        return Ok(None);
    }

    Ok(Some(envelope.value))
}

pub fn cache_put(key: &str, ttl_seconds: u64, value: &Value) -> Result<(), KagiError> {
    let path = cache_response_path(key);
    ensure_parent_dir(&path)?;
    let envelope = CacheEnvelope {
        created_at: now_unix_seconds()?,
        ttl_seconds,
        value: value.clone(),
    };
    write_json(&path, &envelope)
}

pub fn append_history(entry: &HistoryEntry) -> Result<(), KagiError> {
    let path = cache_root().join("history.jsonl");
    ensure_parent_dir(&path)?;
    let mut raw = serde_json::to_string(entry)?;
    raw.push('\n');
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(raw.as_bytes())
        })
        .map_err(|error| {
            KagiError::Config(format!(
                "failed to append history {}: {error}",
                path.display()
            ))
        })
}

pub fn read_history(limit: usize) -> Result<Vec<HistoryEntry>, KagiError> {
    let path = cache_root().join("history.jsonl");
    if !path.exists() {
        return Ok(vec![]);
    }

    let raw = fs::read_to_string(&path).map_err(|error| {
        KagiError::Config(format!(
            "failed to read history {}: {error}",
            path.display()
        ))
    })?;
    let mut entries = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<HistoryEntry>)
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.timestamp);
    entries.reverse();
    if limit > 0 && entries.len() > limit {
        entries.truncate(limit);
    }
    Ok(entries)
}

pub fn history_stats() -> Result<Value, KagiError> {
    let entries = read_history(0)?;
    let mut by_command: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_query: BTreeMap<String, usize> = BTreeMap::new();

    for entry in &entries {
        *by_command.entry(entry.command.clone()).or_default() += 1;
        if let Some(query) = entry
            .query
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            *by_query.entry(query.clone()).or_default() += 1;
        }
    }

    Ok(serde_json::json!({
        "total": entries.len(),
        "by_command": by_command,
        "by_query": by_query,
    }))
}

pub fn load_site_preferences() -> Result<SitePreferences, KagiError> {
    let path = site_preferences_path();
    if !path.exists() {
        return Ok(SitePreferences::default());
    }
    let raw = fs::read_to_string(&path).map_err(|error| {
        KagiError::Config(format!(
            "failed to read site preferences {}: {error}",
            path.display()
        ))
    })?;
    serde_json::from_str(&raw).map_err(|error| {
        KagiError::Parse(format!(
            "failed to parse site preferences {}: {error}",
            path.display()
        ))
    })
}

pub fn save_site_preferences(preferences: &SitePreferences) -> Result<(), KagiError> {
    let path = site_preferences_path();
    ensure_parent_dir(&path)?;
    write_json(&path, preferences)
}

pub fn normalize_domain(input: &str) -> Result<String, KagiError> {
    let trimmed = input
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_matches('/');
    let domain = trimmed
        .split('/')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if domain.is_empty() || domain.contains(char::is_whitespace) {
        return Err(KagiError::Config(format!("invalid domain `{input}`")));
    }
    Ok(domain)
}

fn default_cache_root() -> PathBuf {
    env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_CACHE_SUBDIR)
}

fn cache_response_path(key: &str) -> PathBuf {
    cache_root().join("responses").join(format!("{key}.json"))
}

fn site_preferences_path() -> PathBuf {
    cache_root().join("site-preferences.json")
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), KagiError> {
    let raw = serde_json::to_string_pretty(value)?;
    fs::write(path, raw)
        .map_err(|error| KagiError::Config(format!("failed to write {}: {error}", path.display())))
}

fn ensure_parent_dir(path: &Path) -> Result<(), KagiError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            KagiError::Config(format!("failed to create {}: {error}", parent.display()))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::lock_env;
    use tempfile::TempDir;

    #[test]
    fn cache_round_trips_values() {
        let _guard = lock_env();
        let tempdir = TempDir::new().expect("tempdir");
        unsafe { env::set_var(CACHE_DIR_ENV, tempdir.path()) };

        cache_put("abc", 60, &serde_json::json!({"ok": true})).expect("cache put");
        let value = cache_get("abc").expect("cache get").expect("cached value");

        assert_eq!(value["ok"], true);
        unsafe { env::remove_var(CACHE_DIR_ENV) };
    }

    #[test]
    fn normalizes_domains() {
        assert_eq!(
            normalize_domain("https://Example.COM/path").unwrap(),
            "example.com"
        );
        assert!(normalize_domain(" ").is_err());
    }
}
