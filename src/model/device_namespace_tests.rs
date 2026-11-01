use super::*;
use crate::store::Store;
use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);

#[test]
fn device_namespace_is_not_a_relative_launch_path() {
    // A volume GUID and GLOBALROOT are Windows device namespaces, not drive
    // aliases. Removing their prefix can collide with a relative target.
    for target in [
        r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\Tools\RareApp.exe",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\Tools\RareApp.exe",
    ] {
        let relative = target.strip_prefix(r"\\?\").unwrap();
        assert_ne!(norm(target), norm(relative));
        assert_ne!(target_identity(target), target_identity(relative));
        let mut device = App {
            target: target.into(),
            ..Default::default()
        };
        let mut other = App {
            target: relative.into(),
            ..Default::default()
        };
        device.assign_id();
        other.assign_id();
        assert_ne!(device.id, other.id);
    }
    // Known alternate spellings of the same ordinary drive or UNC path
    // must still resolve to the existing shortcut and policy.
    assert_eq!(
        norm(r"\\?\C:\Tools\RareApp.exe"),
        norm(r"c:\tools\rareapp.exe")
    );
    assert_eq!(
        norm(r"\\?\UNC\Server\Share\RareApp.exe"),
        norm(r"\\server\share\rareapp.exe")
    );
}

#[test]
fn catalog_preserves_two_rules_when_a_device_path_resembles_a_relative_path() {
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-device-identity-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&root).unwrap();
    let mut important = App {
        target: r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\Tools\RareApp.exe".into(),
        source: r"C:\Originals\RareApp.lnk".into(),
        policy: Policy::Always,
        ..Default::default()
    };
    let mut other = App {
        target: r"Volume{01234567-89ab-cdef-0123-456789abcdef}\Tools\RareApp.exe".into(),
        source: r"C:\Other\RareApp.lnk".into(),
        policy: Policy::Never,
        ..Default::default()
    };
    important.assign_id();
    other.assign_id();
    store.put_app(&important).unwrap();
    store.put_app(&other).unwrap();
    let catalog = store.apps().unwrap();
    assert_eq!(
        catalog.len(),
        2,
        "unrelated shortcuts must not overwrite one another"
    );
    assert_eq!(store.app(&important.id).unwrap().policy, Policy::Always);
    assert_eq!(store.app(&other.id).unwrap().policy, Policy::Never);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
