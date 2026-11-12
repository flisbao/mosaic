#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/release_server.rs"]
mod release_server;

use chrono::{Duration, NaiveDate, NaiveDateTime};
use mosaic::application::ports::{Clock, UpdateCheckStore};
use mosaic::application::update::newer_version_for_notice;
use mosaic::domain::release::ReleaseVersion;
use mosaic::infrastructure::curl_release_source::CurlReleaseSource;
use mosaic::infrastructure::update_check_file::FileUpdateCheckStore;
use release_server::ReleaseServer;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;

struct SteppedClock(Mutex<NaiveDateTime>);

impl SteppedClock {
    fn advance(&self, duration: Duration) {
        *self.0.lock().unwrap() += duration;
    }
}

impl Clock for SteppedClock {
    fn now(&self) -> NaiveDateTime {
        *self.0.lock().unwrap()
    }
}

fn private_tempdir() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

fn version(text: &str) -> ReleaseVersion {
    ReleaseVersion::parse(text).unwrap()
}

fn latest_requests(server: &ReleaseServer) -> usize {
    server
        .requests()
        .iter()
        .filter(|path| path.as_str() == "/releases/latest")
        .count()
}

#[test]
fn the_panel_check_asks_the_server_at_most_once_per_day() {
    let server = ReleaseServer::start();
    server.set_latest("0.0.11");
    let directory = private_tempdir();
    let store = FileUpdateCheckStore::new(directory.path().join("estado/update-check.json"));
    let source = CurlReleaseSource::new(Some(server.base()));
    let clock = SteppedClock(Mutex::new(
        NaiveDate::from_ymd_opt(2026, 11, 12)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap(),
    ));
    let current = version("0.0.10");

    assert_eq!(
        newer_version_for_notice(&source, &store, &clock, &current),
        Some(version("0.0.11"))
    );
    assert_eq!(latest_requests(&server), 1);

    clock.advance(Duration::hours(23));
    assert_eq!(
        newer_version_for_notice(&source, &store, &clock, &current),
        Some(version("0.0.11"))
    );
    assert_eq!(latest_requests(&server), 1);

    clock.advance(Duration::hours(2));
    server.set_latest("0.0.12");
    assert_eq!(
        newer_version_for_notice(&source, &store, &clock, &current),
        Some(version("0.0.12"))
    );
    assert_eq!(latest_requests(&server), 2);
    assert_eq!(store.load().unwrap().latest, version("0.0.12"));
}

#[test]
fn an_unreachable_server_is_silent_and_counts_as_the_attempt_of_the_day() {
    let directory = private_tempdir();
    let store = FileUpdateCheckStore::new(directory.path().join("update-check.json"));
    let source = CurlReleaseSource::new(Some("http://127.0.0.1:1/releases".into()));
    let clock = SteppedClock(Mutex::new(
        NaiveDate::from_ymd_opt(2026, 11, 12)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap(),
    ));
    assert_eq!(
        newer_version_for_notice(&source, &store, &clock, &version("0.0.10")),
        None
    );
    assert_eq!(store.load().unwrap().latest, version("0.0.10"));
}
