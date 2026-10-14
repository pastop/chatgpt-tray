use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::state::UsageSnapshot;

const VERSION: u32 = 1;
const FILE_NAME: &str = "usage-cache.json";
const MIN_EPOCH: u64 = 946_684_800; // 2000-01-01
const MAX_EPOCH: u64 = 4_102_444_800; // 2100-01-01
const MAX_BYTES: u64 = 4096;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CachedUsage {
    version: u32,
    pub(crate) usage: UsageSnapshot,
    pub(crate) updated_at_epoch: u64,
    pub(crate) updated_at: String,
}

impl CachedUsage {
    pub(crate) fn new(usage: UsageSnapshot, updated_at_epoch: u64, updated_at: String) -> Self {
        Self {
            version: VERSION,
            usage,
            updated_at_epoch,
            updated_at,
        }
    }

    fn valid(&self, now: u64) -> bool {
        let time = self.updated_at.as_bytes();
        let valid_time = time.len() == 8
            && time[2] == b':'
            && time[5] == b':'
            && [0, 1, 3, 4, 6, 7].iter().all(|&i| time[i].is_ascii_digit())
            && &time[0..2] <= b"23".as_slice()
            && &time[3..5] <= b"59".as_slice()
            && &time[6..8] <= b"59".as_slice();
        self.version == VERSION
            && (MIN_EPOCH..=MAX_EPOCH).contains(&self.updated_at_epoch)
            && self.updated_at_epoch <= now
            && valid_time
            && [&self.usage.primary, &self.usage.secondary]
                .into_iter()
                .flatten()
                .all(|window| {
                    window.remaining <= 100
                        && window.reset_at >= MIN_EPOCH as i64
                        && window.reset_at <= MAX_EPOCH as i64
                        && window.reset_at
                            <= self.updated_at_epoch.saturating_add(366 * 86_400) as i64
                })
    }
}

fn load(directory: &Path, now: u64) -> io::Result<Option<CachedUsage>> {
    let file = match File::open(directory.join(FILE_NAME)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Ok(None);
    }
    Ok(serde_json::from_slice::<CachedUsage>(&bytes)
        .ok()
        .filter(|cache| cache.valid(now)))
}

fn save(directory: &Path, cache: &CachedUsage, now: u64) -> io::Result<()> {
    if !cache.valid(now) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid usage cache",
        ));
    }
    let bytes = serde_json::to_vec(cache)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid usage cache"))?;
    fs::create_dir_all(directory)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(
        "{FILE_NAME}.{}.{nanos}.{sequence}.tmp",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, directory.join(FILE_NAME))?;
        Ok(())
    })();
    if result.is_err() {
        // Never log file contents or parser errors (which can contain input data).
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(crate) async fn load_async(directory: PathBuf, now: u64) -> Option<CachedUsage> {
    match tokio::task::spawn_blocking(move || load(&directory, now)).await {
        Ok(Ok(cache)) => cache,
        _ => {
            eprintln!("Не удалось загрузить кеш статистики");
            None
        }
    }
}

pub(crate) async fn save_async(directory: PathBuf, cache: CachedUsage, now: u64) {
    if !matches!(
        tokio::task::spawn_blocking(move || save(&directory, &cache, now)).await,
        Ok(Ok(()))
    ) {
        eprintln!("Не удалось сохранить кеш статистики");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::WindowSnapshot;

    fn fixture() -> CachedUsage {
        CachedUsage::new(
            UsageSnapshot {
                primary: Some(WindowSnapshot {
                    remaining: 100,
                    reset_at: 1_800_001_000,
                }),
                secondary: None,
            },
            1_800_000_000,
            "12:34:56".into(),
        )
    }

    #[test]
    fn serialization_and_version() {
        let value = serde_json::to_value(fixture()).unwrap();
        let keys: Vec<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["updatedAt", "updatedAtEpoch", "usage", "version"]);
        assert_eq!(value["version"], VERSION);
        let parsed: CachedUsage = serde_json::from_value(value.clone()).unwrap();
        assert!(parsed.valid(1_800_000_000));
        let mut unknown = value;
        unknown["version"] = 2.into();
        assert!(
            !serde_json::from_value::<CachedUsage>(unknown)
                .unwrap()
                .valid(1_800_000_000)
        );
    }

    #[test]
    fn corrupt_and_strict_validation() {
        assert!(serde_json::from_str::<CachedUsage>("{broken").is_err());
        let original = serde_json::to_value(fixture()).unwrap();
        for (field, value) in [
            ("updatedAtEpoch", serde_json::json!(0)),
            ("updatedAtEpoch", serde_json::json!(1_800_000_001_u64)),
            ("updatedAt", serde_json::json!("99:00:00")),
            ("token", serde_json::json!("secret")),
        ] {
            let mut invalid = original.clone();
            invalid[field] = value;
            assert!(
                !serde_json::from_value::<CachedUsage>(invalid)
                    .is_ok_and(|c| c.valid(1_800_000_000))
            );
        }
        for value in [
            serde_json::json!(-1),
            serde_json::json!(101),
            serde_json::json!(1.5),
            serde_json::json!("NaN"),
        ] {
            let mut invalid = original.clone();
            invalid["usage"]["primary"]["remaining"] = value;
            assert!(
                !serde_json::from_value::<CachedUsage>(invalid)
                    .is_ok_and(|c| c.valid(1_800_000_000))
            );
        }
        for timestamp in [-1, 0, i64::MAX, 1_900_000_000] {
            let mut cache = fixture();
            cache.usage.primary.as_mut().unwrap().reset_at = timestamp;
            assert!(!cache.valid(1_800_000_000));
        }
        let mut past = fixture();
        past.usage.primary.as_mut().unwrap().reset_at = 1_700_000_000;
        assert!(past.valid(1_800_000_000));
    }

    #[test]
    fn stale_pid_temporary_does_not_block_save() {
        let directory = std::env::temp_dir().join(format!(
            "usage-cache-stale-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let stale = directory.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
        fs::write(&stale, b"left over after crash").unwrap();

        let cache = fixture();
        save(&directory, &cache, cache.updated_at_epoch).unwrap();
        let loaded = load(&directory, cache.updated_at_epoch).unwrap().unwrap();
        assert_eq!(loaded.updated_at_epoch, cache.updated_at_epoch);
        assert_eq!(loaded.usage.primary.unwrap().remaining, 100);
        assert_eq!(fs::read(&stale).unwrap(), b"left over after crash");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn atomic_roundtrip_and_missing_corrupt_files() {
        let directory = std::env::temp_dir().join(format!(
            "usage-cache-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(load(&directory, 1_800_000_000).unwrap().is_none());
        let mut cache = fixture();
        save(&directory, &cache, 1_800_000_000).unwrap();
        cache.usage.primary.as_mut().unwrap().remaining = 0;
        save(&directory, &cache, 1_800_000_000).unwrap();
        let loaded = load(&directory, 1_800_000_000).unwrap().unwrap();
        assert_eq!(loaded.usage.primary.unwrap().remaining, 0);
        assert_eq!(loaded.updated_at_epoch, cache.updated_at_epoch);
        assert_eq!(loaded.updated_at, cache.updated_at);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::write(directory.join(FILE_NAME), b"corrupt").unwrap();
        assert!(load(&directory, 1_800_000_000).unwrap().is_none());
        fs::remove_dir_all(directory).unwrap();
    }
}
