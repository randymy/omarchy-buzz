//! Durable request-to-event metadata only. No message bodies, keys, or text digests.
use rustix::fs::{self, AtFlags, FlockOperation, Mode, OFlags};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::File,
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Component, PathBuf},
};
const MAX_BYTES: usize = 256 * 1024;
const MAX_RECORDS: usize = 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pending,
    Acknowledged,
    Rejected,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub request_id: String,
    pub origin: String,
    pub identity: String,
    pub room: String,
    pub event_id: String,
    pub outcome: Outcome,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    records: Vec<Record>,
}

pub struct Ledger {
    parent: File,
    filename: std::ffi::OsString,
    _lock: File,
    records: BTreeMap<String, Record>,
    failed: bool,
}

pub fn default_path() -> Result<PathBuf, &'static str> {
    let base = match std::env::var_os("XDG_STATE_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(std::env::var_os("HOME").ok_or("ledger_unavailable")?)
            .join(".local/state"),
    };
    if !base.is_absolute() {
        return Err("ledger_unsafe_path");
    }
    Ok(base.join("omarchy-buzz/delivery/ledger.json"))
}
fn validate(record: &Record) -> Result<(), &'static str> {
    for value in [&record.request_id, &record.room] {
        if uuid::Uuid::parse_str(value)
            .map_err(|_| "ledger_invalid_record")?
            .to_string()
            != *value
        {
            return Err("ledger_invalid_record");
        }
    }
    if record.origin.len() > 2048
        || crate::config::canonical_relay(&record.origin).map_err(|_| "ledger_invalid_record")?
            != record.origin
    {
        return Err("ledger_invalid_record");
    }
    if nostr::PublicKey::from_hex(&record.identity)
        .map_err(|_| "ledger_invalid_record")?
        .to_hex()
        != record.identity
        || nostr::EventId::from_hex(&record.event_id)
            .map_err(|_| "ledger_invalid_record")?
            .to_hex()
            != record.event_id
    {
        return Err("ledger_invalid_record");
    }
    Ok(())
}
fn safe_file(file: &File) -> Result<(), &'static str> {
    let m = file.metadata().map_err(|_| "ledger_unavailable")?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o777 != 0o600
        || m.nlink() != 1
    {
        return Err("ledger_unsafe_path");
    }
    Ok(())
}
fn private_parent(path: &std::path::Path) -> Result<File, &'static str> {
    if !path.is_absolute() {
        return Err("ledger_unsafe_path");
    }
    let mut directory = File::open("/").map_err(|_| "ledger_unavailable")?;
    for component in path.components() {
        let Component::Normal(name) = component else {
            if component == Component::RootDir {
                continue;
            }
            return Err("ledger_unsafe_path");
        };
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd = match fs::openat(&directory, name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => {
                fs::mkdirat(&directory, name, Mode::from_bits_truncate(0o700))
                    .map_err(|_| "ledger_unavailable")?;
                directory.sync_all().map_err(|_| "ledger_unavailable")?;
                fs::openat(&directory, name, flags, Mode::empty())
                    .map_err(|_| "ledger_unsafe_path")?
            }
            Err(_) => return Err("ledger_unsafe_path"),
        };
        directory = File::from(fd);
        let m = directory.metadata().map_err(|_| "ledger_unavailable")?;
        let uid = rustix::process::geteuid().as_raw();
        // Root-owned sticky ancestors (e.g. /tmp) are valid test locations.
        // Every other ancestor must exclude group/other mutation.
        if (m.uid() != uid && m.uid() != 0)
            || (m.mode() & 0o022 != 0 && !(m.uid() == 0 && m.mode() & 0o1000 != 0))
        {
            return Err("ledger_unsafe_path");
        }
    }
    let m = directory.metadata().map_err(|_| "ledger_unavailable")?;
    if m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o777 != 0o700 {
        return Err("ledger_unsafe_path");
    }
    directory.sync_all().map_err(|_| "ledger_unavailable")?;
    Ok(directory)
}
fn open_file(parent: &File, name: &OsStr, create: bool) -> Result<File, &'static str> {
    let mut flags = OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    if create {
        flags |= OFlags::CREATE;
    }
    let file = File::from(
        fs::openat(parent, name, flags, Mode::from_bits_truncate(0o600))
            .map_err(|_| "ledger_unsafe_path")?,
    );
    safe_file(&file)?;
    Ok(file)
}
impl Ledger {
    pub fn open(path: PathBuf) -> Result<Self, &'static str> {
        let parent = private_parent(path.parent().ok_or("ledger_unsafe_path")?)?;
        let filename = path.file_name().ok_or("ledger_unsafe_path")?.to_os_string();
        if filename == ".lock" {
            return Err("ledger_unsafe_path");
        }
        let lock = open_file(&parent, OsStr::new(".lock"), true)?;
        fs::flock(&lock, FlockOperation::NonBlockingLockExclusive).map_err(|_| "ledger_busy")?;
        let mut records = BTreeMap::new();
        match fs::statat(&parent, &filename, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => {
                let mut file = open_file(&parent, &filename, false)?;
                if file.metadata().map_err(|_| "ledger_unavailable")?.len() > MAX_BYTES as u64 {
                    return Err("ledger_oversized");
                }
                let mut bytes = Vec::new();
                Read::by_ref(&mut file)
                    .take((MAX_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "ledger_unavailable")?;
                if bytes.len() > MAX_BYTES {
                    return Err("ledger_oversized");
                }
                let doc: Document =
                    serde_json::from_slice(&bytes).map_err(|_| "ledger_invalid_data")?;
                if doc.version != 1 || doc.records.len() > MAX_RECORDS {
                    return Err("ledger_invalid_data");
                }
                for mut record in doc.records {
                    validate(&record)?;
                    if record.outcome == Outcome::Pending {
                        record.outcome = Outcome::Unknown;
                    }
                    if records.insert(record.request_id.clone(), record).is_some() {
                        return Err("ledger_invalid_data");
                    }
                }
            }
            Err(rustix::io::Errno::NOENT) => {}
            Err(_) => return Err("ledger_unsafe_path"),
        }
        Ok(Self {
            parent,
            filename,
            _lock: lock,
            records,
            failed: false,
        })
    }
    pub fn lookup(&self, request_id: &str) -> Option<Record> {
        self.records.get(request_id).cloned()
    }
    pub fn reserve(&mut self, record: Record) -> Result<(), &'static str> {
        if self.failed {
            return Err("ledger_unavailable");
        }
        validate(&record)?;
        if record.outcome != Outcome::Pending {
            return Err("ledger_invalid_record");
        }
        if self.records.contains_key(&record.request_id) {
            return Err("ledger_duplicate_request");
        }
        if self.records.len() >= MAX_RECORDS {
            return Err("ledger_capacity");
        }
        let mut next = self.records.clone();
        next.insert(record.request_id.clone(), record);
        self.commit(next)
    }
    pub fn outcome(&mut self, request_id: &str, outcome: Outcome) -> Result<(), &'static str> {
        if self.failed {
            return Err("ledger_unavailable");
        }
        if outcome == Outcome::Pending {
            return Err("ledger_invalid_outcome");
        }
        let mut next = self.records.clone();
        let record = next.get_mut(request_id).ok_or("ledger_unknown_request")?;
        if matches!(record.outcome, Outcome::Acknowledged | Outcome::Rejected)
            && record.outcome != outcome
        {
            return Err("ledger_invalid_outcome");
        }
        record.outcome = outcome;
        self.commit(next)
    }
    fn commit(&mut self, next: BTreeMap<String, Record>) -> Result<(), &'static str> {
        let doc = Document {
            version: 1,
            records: next.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec(&doc).map_err(|_| "ledger_invalid_data")?;
        // Byte budget can be exhausted before the independent record ceiling.
        if bytes.len() > MAX_BYTES {
            return Err("ledger_capacity");
        }
        if self.persist(&bytes).is_err() {
            self.failed = true;
            return Err("ledger_unavailable");
        }
        self.records = next;
        Ok(())
    }
    fn persist(&self, bytes: &[u8]) -> Result<(), &'static str> {
        let m = self.parent.metadata().map_err(|_| "ledger_unavailable")?;
        if m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o777 != 0o700 {
            return Err("ledger_unsafe_path");
        }
        safe_file(&self._lock)?;
        match fs::statat(&self.parent, &self.filename, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => {
                open_file(&self.parent, &self.filename, false)?;
            }
            Err(rustix::io::Errno::NOENT) => {}
            Err(_) => return Err("ledger_unsafe_path"),
        }
        let name = format!(".ledger-{}.tmp", uuid::Uuid::new_v4());
        let mut file = File::from(
            fs::openat(
                &self.parent,
                name.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_bits_truncate(0o600),
            )
            .map_err(|_| "ledger_unavailable")?,
        );
        let result = (|| {
            file.write_all(bytes).map_err(|_| "ledger_unavailable")?;
            file.sync_all().map_err(|_| "ledger_unavailable")?;
            fs::renameat(&self.parent, name.as_str(), &self.parent, &self.filename)
                .map_err(|_| "ledger_unavailable")?;
            self.parent.sync_all().map_err(|_| "ledger_unavailable")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::unlinkat(&self.parent, name.as_str(), AtFlags::empty());
        }
        result
    }
}
#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
