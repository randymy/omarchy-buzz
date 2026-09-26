use super::*;
use std::{
    fs as disk,
    os::unix::fs::{symlink, PermissionsExt},
};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("omarchy-buzz-ledger-{}", uuid::Uuid::new_v4()));
        disk::create_dir(&path).unwrap();
        disk::set_permissions(&path, disk::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("delivery/ledger.json")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = disk::remove_dir_all(&self.0);
    }
}
fn record(n: u128) -> Record {
    Record {
        request_id: uuid::Uuid::from_u128(n).to_string(),
        origin: "ws://127.0.0.1:30001/".into(),
        identity: nostr::Keys::parse(&format!("{:064x}", 1))
            .unwrap()
            .public_key()
            .to_hex(),
        room: uuid::Uuid::from_u128(42).to_string(),
        event_id: format!("{n:064x}"),
        outcome: Outcome::Pending,
    }
}
#[test]
fn restart_preserves_binding_and_never_reserves_an_existing_request_again() {
    let temp = Temp::new();
    let r = record(1);
    {
        let mut ledger = Ledger::open(temp.path()).unwrap();
        ledger.reserve(r.clone()).unwrap();
        assert_eq!(ledger.lookup(&r.request_id), Some(r.clone()));
    }
    let mut ledger = Ledger::open(temp.path()).unwrap();
    let loaded = ledger.lookup(&r.request_id).unwrap();
    assert_eq!(loaded.event_id, r.event_id);
    assert_eq!(loaded.outcome, Outcome::Unknown);
    let mut conflict = r.clone();
    conflict.event_id = record(2).event_id;
    assert_eq!(
        ledger.reserve(conflict).unwrap_err(),
        "ledger_duplicate_request"
    );
    ledger
        .outcome(&r.request_id, Outcome::Acknowledged)
        .unwrap();
    drop(ledger);
    let mut ledger = Ledger::open(temp.path()).unwrap();
    assert_eq!(
        ledger.lookup(&r.request_id).unwrap().outcome,
        Outcome::Acknowledged
    );
    assert_eq!(
        ledger.outcome(&r.request_id, Outcome::Unknown).unwrap_err(),
        "ledger_invalid_outcome"
    );
    assert_eq!(ledger.reserve(r).unwrap_err(), "ledger_duplicate_request");
}
#[test]
fn lock_is_exclusive_and_released_with_the_ledger() {
    let temp = Temp::new();
    let ledger = Ledger::open(temp.path()).unwrap();
    assert!(matches!(Ledger::open(temp.path()), Err("ledger_busy")));
    drop(ledger);
    assert!(Ledger::open(temp.path()).is_ok());
}
#[test]
fn storage_is_private_and_contains_only_the_permitted_metadata() {
    let temp = Temp::new();
    let mut ledger = Ledger::open(temp.path()).unwrap();
    let r = record(3);
    ledger.reserve(r.clone()).unwrap();
    assert_eq!(
        disk::metadata(temp.path().parent().unwrap())
            .unwrap()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(disk::metadata(temp.path()).unwrap().mode() & 0o777, 0o600);
    let bytes = disk::read(temp.path()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let fields = json["records"][0].as_object().unwrap();
    assert_eq!(fields.len(), 6);
    for field in [
        "request_id",
        "origin",
        "identity",
        "room",
        "event_id",
        "outcome",
    ] {
        assert!(fields.contains_key(field));
    }
    for forbidden in [
        "synthetic message plaintext",
        "private_key",
        "content",
        "text_hash",
        "signed_event",
    ] {
        assert!(!String::from_utf8_lossy(&bytes).contains(forbidden));
    }
    assert_eq!(json["records"][0]["outcome"], "pending");
    assert_eq!(
        disk::read_dir(temp.path().parent().unwrap())
            .unwrap()
            .count(),
        2,
        "atomic update left temporary files"
    );
}
#[test]
fn unsafe_modes_symlinks_and_hardlinks_are_rejected() {
    let temp = Temp::new();
    let path = temp.path();
    disk::create_dir(path.parent().unwrap()).unwrap();
    disk::set_permissions(path.parent().unwrap(), disk::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        Ledger::open(path.clone()),
        Err("ledger_unsafe_path")
    ));
    disk::set_permissions(path.parent().unwrap(), disk::Permissions::from_mode(0o700)).unwrap();
    let external = temp.0.join("external");
    disk::write(&external, b"{}").unwrap();
    disk::set_permissions(&external, disk::Permissions::from_mode(0o600)).unwrap();
    symlink(&external, &path).unwrap();
    assert!(matches!(
        Ledger::open(path.clone()),
        Err("ledger_unsafe_path")
    ));
    disk::remove_file(&path).unwrap();
    disk::hard_link(&external, &path).unwrap();
    assert!(matches!(
        Ledger::open(path.clone()),
        Err("ledger_unsafe_path")
    ));
    disk::remove_file(&path).unwrap();
    let linked = temp.0.join("linked");
    symlink(path.parent().unwrap(), &linked).unwrap();
    assert!(matches!(
        Ledger::open(linked.join("ledger.json")),
        Err("ledger_unsafe_path")
    ));
    disk::remove_file(path.parent().unwrap().join(".lock")).unwrap();
    symlink(&external, path.parent().unwrap().join(".lock")).unwrap();
    assert!(matches!(Ledger::open(path), Err("ledger_unsafe_path")));
}
#[test]
fn failed_persistence_poison_prevents_further_send_reservations() {
    let temp = Temp::new();
    let mut ledger = Ledger::open(temp.path()).unwrap();
    let r = record(4);
    ledger.reserve(r.clone()).unwrap();
    disk::set_permissions(temp.path(), disk::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        ledger
            .outcome(&r.request_id, Outcome::Acknowledged)
            .unwrap_err(),
        "ledger_unavailable"
    );
    disk::set_permissions(temp.path(), disk::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(ledger.reserve(record(5)).unwrap_err(), "ledger_unavailable");
    assert_eq!(
        ledger
            .outcome(&r.request_id, Outcome::Acknowledged)
            .unwrap_err(),
        "ledger_unavailable"
    );
    assert_eq!(
        ledger.lookup(&r.request_id).unwrap().outcome,
        Outcome::Pending
    );
}
#[test]
fn byte_capacity_is_restart_durable_and_resolved_records_are_not_pruned() {
    let temp = Temp::new();
    let mut ledger = Ledger::open(temp.path()).unwrap();
    ledger.reserve(record(1)).unwrap();
    ledger
        .outcome(&record(1).request_id, Outcome::Acknowledged)
        .unwrap();
    let mut count = 1;
    loop {
        match ledger.reserve(record(count + 1)) {
            Ok(()) => count += 1,
            Err("ledger_capacity") => break,
            Err(error) => panic!("unexpected reservation failure {error}"),
        }
    }
    assert!(count > 0 && count <= MAX_RECORDS as u128);
    assert!(disk::metadata(temp.path()).unwrap().len() <= MAX_BYTES as u64);
    drop(ledger);
    let mut ledger = Ledger::open(temp.path()).unwrap();
    assert_eq!(ledger.records.len(), count as usize);
    assert_eq!(
        ledger.reserve(record(count + 1)).unwrap_err(),
        "ledger_capacity"
    );
    assert_eq!(
        ledger.reserve(record(1)).unwrap_err(),
        "ledger_duplicate_request"
    );
}
#[test]
fn record_count_ceiling_and_invalid_records_refuse_without_writing() {
    let temp = Temp::new();
    let mut ledger = Ledger::open(temp.path()).unwrap();
    let mut invalid = record(1);
    invalid.identity = "not a public key".into();
    assert_eq!(
        ledger.reserve(invalid).unwrap_err(),
        "ledger_invalid_record"
    );
    assert!(!temp.path().exists());
    for n in 1..=MAX_RECORDS as u128 {
        let r = record(n);
        ledger.records.insert(r.request_id.clone(), r);
    }
    assert_eq!(
        ledger.reserve(record(MAX_RECORDS as u128 + 1)).unwrap_err(),
        "ledger_capacity"
    );
    assert!(!temp.path().exists());
}
#[test]
fn oversized_invalid_or_extended_disk_documents_fail_closed() {
    let temp = Temp::new();
    {
        let mut ledger = Ledger::open(temp.path()).unwrap();
        ledger.reserve(record(1)).unwrap();
    }
    let mut doc: serde_json::Value =
        serde_json::from_slice(&disk::read(temp.path()).unwrap()).unwrap();
    doc["records"][0]["content"] = serde_json::json!("synthetic message plaintext");
    disk::write(temp.path(), serde_json::to_vec(&doc).unwrap()).unwrap();
    assert!(matches!(
        Ledger::open(temp.path()),
        Err("ledger_invalid_data")
    ));
    disk::write(temp.path(), vec![b' '; MAX_BYTES + 1]).unwrap();
    assert!(matches!(Ledger::open(temp.path()), Err("ledger_oversized")));
    disk::write(temp.path(), b"invalid").unwrap();
    assert!(matches!(
        Ledger::open(temp.path()),
        Err("ledger_invalid_data")
    ));
}
#[test]
fn reservation_io_failure_is_poisoned_and_never_reserves_the_request() {
    let temp = Temp::new();
    let mut ledger = Ledger::open(temp.path()).unwrap();
    let target = temp.0.join("outside-ledger");
    disk::write(&target, b"untouched synthetic fixture").unwrap();
    disk::set_permissions(&target, disk::Permissions::from_mode(0o600)).unwrap();
    symlink(&target, temp.path()).unwrap();
    let r = record(7);
    assert_eq!(ledger.reserve(r.clone()).unwrap_err(), "ledger_unavailable");
    assert!(ledger.lookup(&r.request_id).is_none());
    assert_eq!(disk::read(&target).unwrap(), b"untouched synthetic fixture");
    disk::remove_file(temp.path()).unwrap();
    assert_eq!(ledger.reserve(r).unwrap_err(), "ledger_unavailable");
    assert!(!temp.path().exists());
}
