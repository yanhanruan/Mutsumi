//! Opt-in live smoke tests for macOS system integration backends.
//!
//! These tests are ignored by the ordinary suite because they briefly write to
//! the user's Keychain or `~/Library/LaunchAgents`. Each test uses an isolated,
//! process-unique identifier and removes its temporary item before returning.

use auto_launch::AutoLaunch;
use keyring::Entry;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

struct KeychainCleanup {
    entry: Entry,
    service: String,
}

impl Drop for KeychainCleanup {
    fn drop(&mut self) {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => eprintln!(
                "failed to remove temporary Keychain item for service {}: {error}",
                self.service
            ),
        }
    }
}

struct LaunchAgentCleanup {
    backend: AutoLaunch,
    app_name: String,
}

impl Drop for LaunchAgentCleanup {
    fn drop(&mut self) {
        if let Err(error) = self.backend.disable() {
            eprintln!(
                "failed to remove temporary LaunchAgent plist for {}: {error}",
                self.app_name
            );
        }
    }
}

#[test]
#[ignore = "writes and removes an isolated temporary macOS Keychain item"]
fn keychain_backend_round_trip() {
    let suffix = unique_suffix();
    let service = format!("com.mutsumi.live-smoke.{suffix}");
    let secret = format!("temporary-secret-{suffix}");
    let entry =
        Entry::new(&service, "temporary-test-account").expect("create isolated Keychain entry");
    let cleanup = KeychainCleanup { entry, service };

    cleanup
        .entry
        .set_password(&secret)
        .expect("write isolated Keychain item");
    assert_eq!(
        cleanup
            .entry
            .get_password()
            .expect("read isolated Keychain item"),
        secret
    );
    cleanup
        .entry
        .delete_credential()
        .expect("remove isolated Keychain item");
    assert!(matches!(
        cleanup.entry.get_password(),
        Err(keyring::Error::NoEntry)
    ));
}

#[test]
#[ignore = "writes and removes an isolated temporary LaunchAgent plist"]
fn launch_agent_backend_round_trip() {
    let app_name = format!("com.mutsumi.live-smoke.{}", unique_suffix());
    let executable = std::env::current_exe().expect("resolve current test executable");
    let executable = executable
        .to_str()
        .expect("test executable path is valid UTF-8");
    let args: [&str; 0] = [];
    let cleanup = LaunchAgentCleanup {
        backend: AutoLaunch::new(&app_name, executable, true, &args),
        app_name,
    };

    assert!(!cleanup
        .backend
        .is_enabled()
        .expect("read initial LaunchAgent state"));
    cleanup
        .backend
        .enable()
        .expect("create isolated LaunchAgent plist");
    assert!(cleanup
        .backend
        .is_enabled()
        .expect("read enabled LaunchAgent state"));
    cleanup
        .backend
        .disable()
        .expect("remove isolated LaunchAgent plist");
    assert!(!cleanup
        .backend
        .is_enabled()
        .expect("read disabled LaunchAgent state"));
}
