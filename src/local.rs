//! Local persistence helpers for cache, history, and site preferences.
//!
//! These helpers intentionally use plain JSON files under the user's cache
//! directory. The issue asks for local, opt-in workflow state; a file-backed
//! store keeps that state inspectable and avoids adding a database dependency
//! before the CLI has enough history volume to justify it.
//!
//! Every file access takes an advisory lock (`fs2`) so concurrent CLI
//! processes cannot interleave writes or read torn files. Mutations that
//! read, modify, and rewrite a file hold one exclusive lock across the whole
//! operation; see [`update_site_preferences`].

use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
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

    let mut file = open_locked_shared(&path)?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).map_err(|error| {
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
    let envelope = CacheEnvelope {
        created_at: now_unix_seconds()?,
        ttl_seconds,
        value: value.clone(),
    };
    write_json_locked(&path, &envelope)
}

pub fn append_history(entry: &HistoryEntry) -> Result<(), KagiError> {
    let path = cache_root().join("history.jsonl");
    ensure_parent_dir(&path)?;
    let mut raw = serde_json::to_string(entry)?;
    raw.push('\n');
    let mut file = open_locked_append(&path)?;
    file.write_all(raw.as_bytes()).map_err(|error| {
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

    let mut file = open_locked_shared(&path)?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).map_err(|error| {
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
    let mut file = open_locked_shared(&path)?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).map_err(|error| {
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

/// Reads, mutates, and rewrites the site preferences file while holding one
/// exclusive advisory lock, so concurrent `site-pref set` and `site-pref
/// remove` invocations cannot silently lose each other's updates. Prefer
/// this over separate [`load_site_preferences`] reads followed by blind
/// rewrites whenever a mutation depends on the previously stored state.
pub fn update_site_preferences<R>(
    update: impl FnOnce(&mut SitePreferences) -> Result<R, KagiError>,
) -> Result<R, KagiError> {
    let path = site_preferences_path();
    ensure_parent_dir(&path)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| {
            KagiError::Config(format!(
                "failed to open site preferences {}: {error}",
                path.display()
            ))
        })?;
    file.lock_exclusive().map_err(|error| {
        KagiError::Config(format!(
            "failed to lock site preferences {}: {error}",
            path.display()
        ))
    })?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).map_err(|error| {
        KagiError::Config(format!(
            "failed to read site preferences {}: {error}",
            path.display()
        ))
    })?;
    let mut preferences = if raw.trim().is_empty() {
        SitePreferences::default()
    } else {
        serde_json::from_str(&raw).map_err(|error| {
            KagiError::Parse(format!(
                "failed to parse site preferences {}: {error}",
                path.display()
            ))
        })?
    };
    let result = update(&mut preferences)?;
    let raw = serde_json::to_string_pretty(&preferences)?;
    file.set_len(0).map_err(|error| {
        KagiError::Config(format!(
            "failed to write site preferences {}: {error}",
            path.display()
        ))
    })?;
    file.rewind().map_err(|error| {
        KagiError::Config(format!(
            "failed to write site preferences {}: {error}",
            path.display()
        ))
    })?;
    file.write_all(raw.as_bytes()).map_err(|error| {
        KagiError::Config(format!(
            "failed to write site preferences {}: {error}",
            path.display()
        ))
    })?;
    Ok(result)
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

/// Serializes `value` as pretty JSON and writes it to `path` while holding
/// an exclusive advisory lock, so readers under a shared lock never observe
/// a partially written file.
fn write_json_locked<T: Serialize>(path: &Path, value: &T) -> Result<(), KagiError> {
    ensure_parent_dir(path)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|error| {
            KagiError::Config(format!("failed to write {}: {error}", path.display()))
        })?;
    file.lock_exclusive().map_err(|error| {
        KagiError::Config(format!("failed to lock {}: {error}", path.display()))
    })?;
    // Truncate only after the lock is held: truncating in `open()` would
    // empty the file while a concurrent shared-lock reader is still reading.
    file.set_len(0).map_err(|error| {
        KagiError::Config(format!("failed to write {}: {error}", path.display()))
    })?;
    serde_json::to_writer_pretty(&file, value)?;
    file.flush()
        .map_err(|error| KagiError::Config(format!("failed to write {}: {error}", path.display())))
}

/// Opens `path` in append mode and acquires an exclusive advisory lock, so
/// one JSON Lines record is always written without interleaving.
fn open_locked_append(path: &Path) -> Result<File, KagiError> {
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            KagiError::Config(format!("failed to append {}: {error}", path.display()))
        })?;
    file.lock_exclusive().map_err(|error| {
        KagiError::Config(format!("failed to lock {}: {error}", path.display()))
    })?;
    Ok(file)
}

/// Opens an existing `path` for reading and acquires a shared advisory lock.
fn open_locked_shared(path: &Path) -> Result<File, KagiError> {
    let file = fs::File::open(path).map_err(|error| {
        KagiError::Config(format!("failed to read {}: {error}", path.display()))
    })?;
    file.lock_shared().map_err(|error| {
        KagiError::Config(format!("failed to lock {}: {error}", path.display()))
    })?;
    Ok(file)
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
    use std::sync::{Arc, Barrier};
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

    #[test]
    fn concurrent_preference_updates_all_survive() {
        let _guard = lock_env();
        let tempdir = TempDir::new().expect("tempdir");
        unsafe { env::set_var(CACHE_DIR_ENV, tempdir.path()) };

        let domains: Vec<String> = (0..16)
            .map(|index| format!("domain-{index}.example"))
            .collect();
        let barrier = Arc::new(Barrier::new(domains.len()));
        let mut handles = Vec::new();
        for domain in domains.clone() {
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                update_site_preferences(|preferences| {
                    preferences
                        .domains
                        .insert(domain.clone(), SitePreferenceMode::Pin);
                    Ok::<(), KagiError>(())
                })
                .expect("concurrent preference update");
            }));
        }
        for handle in handles {
            handle.join().expect("worker thread");
        }

        let stored = load_site_preferences().expect("load preferences");
        for domain in &domains {
            assert_eq!(
                stored.domains.get(domain),
                Some(&SitePreferenceMode::Pin),
                "concurrent update for {domain} must survive"
            );
        }
        unsafe { env::remove_var(CACHE_DIR_ENV) };
    }

    #[test]
    fn concurrent_history_appends_stay_parseable() {
        let _guard = lock_env();
        let tempdir = TempDir::new().expect("tempdir");
        unsafe { env::set_var(CACHE_DIR_ENV, tempdir.path()) };

        let workers = 8;
        let per_worker = 25;
        let barrier = Arc::new(Barrier::new(workers));
        let mut handles = Vec::new();
        for worker in 0..workers {
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                for index in 0..per_worker {
                    append_history(&HistoryEntry {
                        timestamp: (worker * per_worker + index) as u64,
                        command: "search".to_string(),
                        query: Some(format!("worker {worker} query {index}")),
                        result_count: Some(index),
                    })
                    .expect("concurrent history append");
                }
            }));
        }
        for handle in handles {
            handle.join().expect("worker thread");
        }

        let entries = read_history(0).expect("history must stay parseable");
        assert_eq!(entries.len(), workers * per_worker);
        unsafe { env::remove_var(CACHE_DIR_ENV) };
    }
}
