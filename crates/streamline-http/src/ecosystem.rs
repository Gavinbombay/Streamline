//! Read-only homelab health probes. No caller-supplied paths, URLs or command arguments.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use streamline_safe::PinnedDir;

const STATUS_DIR: &str = "/var/lib/streamline/vault";
const STATUS_FILE: &str = "ECOSYSTEM-STATUS.md";
const STATUS_PATH: &str = "/var/lib/streamline/vault/ECOSYSTEM-STATUS.md";
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_STATUS_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Probe {
    Fleet,
    BackupTimer,
    BackupService,
    FailedUnits,
    RootFilesystem,
}

impl Probe {
    fn command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Fleet => (
                "/usr/bin/curl",
                &[
                    "-q", "--noproxy", "*", "--silent", "--fail", "--max-time", "2",
                    "--max-filesize", "262144", "http://127.0.0.1:5820/health",
                ],
            ),
            Self::BackupTimer => (
                "/usr/bin/systemctl",
                &[
                    "--no-pager", "show",
                    "--property=LoadState,ActiveState,SubState,LastTriggerUSec,NextElapseUSecRealtime",
                    "nuc-offhost-backup.timer",
                ],
            ),
            Self::BackupService => (
                "/usr/bin/systemctl",
                &[
                    "--no-pager", "show",
                    "--property=LoadState,ActiveState,SubState,Result,ExecMainStatus,ExecMainExitTimestamp",
                    "nuc-offhost-backup.service",
                ],
            ),
            Self::FailedUnits => (
                "/usr/bin/systemctl",
                &["--no-pager", "--plain", "--failed", "--no-legend"],
            ),
            Self::RootFilesystem => (
                "/usr/bin/df",
                &["-k", "--output=size,used,avail,pcent", "/"],
            ),
        }
    }
}

fn run_probe(probe: Probe) -> Option<String> {
    let (program, args) = probe.command();
    // GNU timeout also bounds systemctl and df. Inherit neither credentials nor
    // proxy/config environment and never return command stderr or raw stdout.
    let mut child = Command::new("/usr/bin/timeout")
        .args(["--kill-after=1s", "2s", program])
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let mut bytes = Vec::new();
    // Drop the pipe at the byte ceiling, even if the peer ignores the limit.
    let read = child.stdout.take().map(|stdout| {
        stdout
            .take(MAX_OUTPUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
    });
    // Always reap the timeout wrapper, including failed or oversized reads.
    let status = child.wait().ok()?;
    if !status.success() || read?.is_err() || bytes.len() > MAX_OUTPUT_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[derive(Deserialize)]
struct Health {
    fleet: Fleet,
}

#[derive(Deserialize)]
struct Fleet {
    fleet_dispatch_allowed: bool,
    workers_total: u64,
    workers_active: u64,
    workers_draining: u64,
    workers_offline: u64,
    capabilities_count: u64,
}

fn fleet_summary(raw: Option<&str>) -> Value {
    let fleet = raw
        .and_then(|text| serde_json::from_str::<Health>(text).ok())
        .map(|health| health.fleet);
    json!({
        "available": fleet.is_some(),
        "fleet_dispatch_allowed": fleet.as_ref().map(|f| f.fleet_dispatch_allowed),
        "workers_total": fleet.as_ref().map(|f| f.workers_total),
        "workers_active": fleet.as_ref().map(|f| f.workers_active),
        "workers_draining": fleet.as_ref().map(|f| f.workers_draining),
        "workers_offline": fleet.as_ref().map(|f| f.workers_offline),
        "capabilities_count": fleet.as_ref().map(|f| f.capabilities_count),
    })
}

fn property<'a>(raw: Option<&'a str>, name: &str) -> Option<&'a str> {
    raw?.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key == name && !value.is_empty()).then_some(value)
    })
}

fn allowed<'a>(value: Option<&'a str>, choices: &[&str]) -> Option<&'a str> {
    value.filter(|value| choices.contains(value))
}

fn timestamp(value: Option<&str>) -> Option<String> {
    // systemctl runs with LC_ALL=C and TZ=UTC. Emit only parsed timestamps.
    let timestamp = NaiveDateTime::parse_from_str(value?, "%a %Y-%m-%d %H:%M:%S UTC").ok()?;
    Some(timestamp.and_utc().to_rfc3339())
}

fn backup_summary(raw: Option<&str>, timer: bool) -> Value {
    let load = allowed(
        property(raw, "LoadState"),
        &[
            "loaded",
            "not-found",
            "bad-setting",
            "error",
            "masked",
            "stub",
            "merged",
        ],
    );
    let active = allowed(
        property(raw, "ActiveState"),
        &[
            "active",
            "reloading",
            "inactive",
            "failed",
            "activating",
            "deactivating",
            "maintenance",
            "refreshing",
        ],
    );
    let sub = allowed(
        property(raw, "SubState"),
        &[
            "dead",
            "waiting",
            "running",
            "elapsed",
            "failed",
            "exited",
            "start-pre",
            "start",
            "start-post",
            "reload",
            "stop",
            "stop-watchdog",
            "stop-sigterm",
            "stop-sigkill",
            "stop-post",
            "final-watchdog",
            "final-sigterm",
            "final-sigkill",
            "auto-restart",
            "auto-restart-queued",
            "cleaning",
            "deactivating",
        ],
    );
    let mut summary = json!({
        "available": load == Some("loaded") && active.is_some() && sub.is_some(),
        "load_state": load,
        "active_state": active,
        "sub_state": sub,
    });
    if timer {
        summary["last_trigger"] = json!(timestamp(property(raw, "LastTriggerUSec")));
        summary["next_elapse"] = json!(timestamp(property(raw, "NextElapseUSecRealtime")));
    } else {
        summary["result"] = json!(allowed(
            property(raw, "Result"),
            &[
                "success",
                "resources",
                "protocol",
                "timeout",
                "exit-code",
                "signal",
                "core-dump",
                "watchdog",
                "start-limit-hit",
                "oom-kill",
                "exec-condition",
                "skip-condition",
            ]
        ));
        summary["exec_main_status"] =
            json!(property(raw, "ExecMainStatus").and_then(|s| s.parse::<u32>().ok()));
        summary["exec_main_exit_timestamp"] =
            json!(timestamp(property(raw, "ExecMainExitTimestamp")));
    }
    summary
}

fn failed_count(raw: &str) -> Option<usize> {
    let mut count = 0;
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        // --plain removes status bullets; UNIT LOAD ACTIVE SUB DESCRIPTION.
        if line.split_whitespace().nth(2) != Some("failed") {
            return None;
        }
        count += 1;
    }
    Some(count)
}

fn filesystem_usage(raw: &str) -> Option<(u64, u64, u64, u64)> {
    let mut lines = raw.lines().filter(|line| !line.trim().is_empty());
    lines.next()?; // Fixed df column header, never returned.
    let mut fields = lines.next()?.split_whitespace();
    let total = fields.next()?.parse().ok()?;
    let used = fields.next()?.parse().ok()?;
    let available = fields.next()?.parse().ok()?;
    let percent = fields.next()?.strip_suffix('%')?.parse().ok()?;
    if fields.next().is_some() || lines.next().is_some() {
        return None;
    }
    Some((total, used, available, percent))
}

struct StatusMetadata {
    last_modified: DateTime<Utc>,
    line_count: usize,
}

fn status_metadata(dir: &Path) -> Option<StatusMetadata> {
    let directory = PinnedDir::open_existing_dir_strict(dir).ok()?;
    let file = directory.open_file_nofollow(STATUS_FILE).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.len() > MAX_STATUS_BYTES as u64 {
        return None;
    }
    let last_modified = DateTime::<Utc>::from(metadata.modified().ok()?);
    let mut bytes = Vec::new();
    file.take(MAX_STATUS_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_STATUS_BYTES {
        return None;
    }
    let line_count = std::str::from_utf8(&bytes).ok()?.lines().count();
    Some(StatusMetadata {
        last_modified,
        line_count,
    })
}

fn collect_with(
    mut run: impl FnMut(Probe) -> Option<String>,
    metadata: Option<StatusMetadata>,
) -> Value {
    let fleet = fleet_summary(run(Probe::Fleet).as_deref());
    let timer = backup_summary(run(Probe::BackupTimer).as_deref(), true);
    let service = backup_summary(run(Probe::BackupService).as_deref(), false);
    let failed = run(Probe::FailedUnits).as_deref().and_then(failed_count);
    let usage = run(Probe::RootFilesystem)
        .as_deref()
        .and_then(filesystem_usage);
    json!({
        "collected_at": Utc::now(),
        "fleet": fleet,
        "backup": { "timer": timer, "service": service },
        "systemd": { "available": failed.is_some(), "failed_units": failed },
        "root_filesystem": {
            "available": usage.is_some(),
            "total_kib": usage.map(|v| v.0),
            "used_kib": usage.map(|v| v.1),
            "available_kib": usage.map(|v| v.2),
            "used_percent": usage.map(|v| v.3),
        },
        "ecosystem_status": {
            "path": STATUS_PATH,
            "available": metadata.is_some(),
            "last_modified": metadata.as_ref().map(|m| m.last_modified),
            "line_count": metadata.as_ref().map(|m| m.line_count),
        },
    })
}

pub(crate) fn collect() -> Value {
    collect_with(run_probe, status_metadata(Path::new(STATUS_DIR)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, DirBuilderExt};
    use std::path::PathBuf;

    const FLEET: &str = r#"{"fleet":{"fleet_dispatch_allowed":false,"workers_total":6,"workers_active":3,"workers_draining":1,"workers_offline":2,"capabilities_count":22,"credential":"fixture-private"},"extra":"fixture-private"}"#;
    const TIMER: &str = "LoadState=loaded\nActiveState=active\nSubState=waiting\nLastTriggerUSec=Thu 2026-11-26 07:00:00 UTC\nNextElapseUSecRealtime=Fri 2026-11-27 07:00:00 UTC\nEnvironment=fixture-private\n";
    const SERVICE: &str = "LoadState=loaded\nActiveState=inactive\nSubState=dead\nResult=exit-code\nExecMainStatus=1\nExecMainExitTimestamp=Thu 2026-11-26 07:02:00 UTC\nExecStart=fixture-private\n";
    const DF: &str = "1K-blocks Used Avail Use%\n1000 250 750 25%\n";

    #[test]
    fn mock_success_returns_only_allowlisted_values() {
        let brief = collect_with(
            |probe| {
                Some(match probe {
                Probe::Fleet => FLEET,
                Probe::BackupTimer => TIMER,
                Probe::BackupService => SERVICE,
                Probe::FailedUnits => "fixture-private.service loaded failed failed Description\nother.service loaded failed failed Other\n",
                Probe::RootFilesystem => DF,
            }.into())
            },
            Some(StatusMetadata {
                last_modified: "2026-11-26T07:00:00Z".parse().unwrap(),
                line_count: 42,
            }),
        );
        assert_eq!(
            brief["fleet"],
            json!({"available":true,"fleet_dispatch_allowed":false,"workers_total":6,"workers_active":3,"workers_draining":1,"workers_offline":2,"capabilities_count":22})
        );
        assert_eq!(
            brief["backup"]["timer"],
            json!({"available":true,"load_state":"loaded","active_state":"active","sub_state":"waiting","last_trigger":"2026-11-26T07:00:00+00:00","next_elapse":"2026-11-27T07:00:00+00:00"})
        );
        assert_eq!(
            brief["backup"]["service"],
            json!({"available":true,"load_state":"loaded","active_state":"inactive","sub_state":"dead","result":"exit-code","exec_main_status":1,"exec_main_exit_timestamp":"2026-11-26T07:02:00+00:00"})
        );
        assert_eq!(brief["systemd"], json!({"available":true,"failed_units":2}));
        assert_eq!(
            brief["root_filesystem"],
            json!({"available":true,"total_kib":1000,"used_kib":250,"available_kib":750,"used_percent":25})
        );
        assert_eq!(
            brief["ecosystem_status"],
            json!({"available":true,"path":STATUS_PATH,"last_modified":"2026-11-26T07:00:00Z","line_count":42})
        );
        assert!(brief["collected_at"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .is_ok());
        assert!(!brief.to_string().contains("fixture-private"));
    }

    #[test]
    fn all_sources_unavailable_keep_stable_shape_and_nulls() {
        let mut calls = Vec::new();
        let brief = collect_with(
            |probe| {
                calls.push(probe);
                None
            },
            None,
        );
        assert_eq!(
            calls,
            [
                Probe::Fleet,
                Probe::BackupTimer,
                Probe::BackupService,
                Probe::FailedUnits,
                Probe::RootFilesystem
            ]
        );
        for section in [
            &brief["fleet"],
            &brief["backup"]["timer"],
            &brief["backup"]["service"],
            &brief["systemd"],
            &brief["root_filesystem"],
            &brief["ecosystem_status"],
        ] {
            assert_eq!(section["available"], false);
            for (key, value) in section.as_object().unwrap() {
                if key != "available" && key != "path" {
                    assert!(value.is_null(), "non-null fallback for {key}");
                }
            }
        }
        assert_eq!(brief["ecosystem_status"]["path"], STATUS_PATH);
    }

    #[test]
    fn partial_failure_does_not_hide_healthy_sources() {
        let brief = collect_with(
            |probe| match probe {
                Probe::Fleet => None,
                Probe::BackupTimer => Some(TIMER.into()),
                Probe::BackupService => {
                    Some("LoadState=not-found\nActiveState=inactive\nSubState=dead\n".into())
                }
                Probe::FailedUnits => Some("\n".into()),
                Probe::RootFilesystem => Some(DF.into()),
            },
            None,
        );
        assert_eq!(brief["fleet"]["available"], false);
        assert_eq!(brief["backup"]["timer"]["available"], true);
        assert_eq!(brief["backup"]["service"]["available"], false);
        assert_eq!(brief["backup"]["service"]["load_state"], "not-found");
        assert_eq!(brief["systemd"]["failed_units"], 0);
        assert_eq!(brief["root_filesystem"]["available"], true);
    }

    #[test]
    fn malformed_fleet_payloads_are_unavailable() {
        for raw in [
            "not JSON",
            "{}",
            r#"{"fleet":null}"#,
            r#"{"fleet":{"workers_total":0}}"#,
        ] {
            assert_eq!(fleet_summary(Some(raw))["available"], false);
        }
        for raw in [
            FLEET.replace("\"workers_total\":6", "\"workers_total\":-1"),
            FLEET.replace("false", "\"false\""),
        ] {
            assert_eq!(fleet_summary(Some(&raw))["available"], false);
        }
    }

    #[test]
    fn backup_parser_rejects_unknown_values_and_handles_unset_timestamps() {
        let raw = "LoadState=loaded\nActiveState=active\nSubState=waiting\nLastTriggerUSec=\nNextElapseUSecRealtime=n/a\n";
        let timer = backup_summary(Some(raw), true);
        assert_eq!(timer["available"], true);
        assert!(timer["last_trigger"].is_null());
        assert!(timer["next_elapse"].is_null());
        let raw = "LoadState=loaded\nActiveState=fixture-private\nSubState=fixture-private\nResult=fixture-private\nExecMainStatus=fixture-private\nExecMainExitTimestamp=fixture-private\n";
        let service = backup_summary(Some(raw), false);
        assert_eq!(service["available"], false);
        assert!(service["result"].is_null());
        assert!(service["exec_main_status"].is_null());
        assert!(service["exec_main_exit_timestamp"].is_null());
        assert!(!service.to_string().contains("fixture-private"));
    }

    #[test]
    fn malformed_systemd_and_filesystem_output_does_not_report_zero() {
        assert_eq!(failed_count(""), Some(0));
        assert_eq!(failed_count("not a unit"), None);
        assert_eq!(
            failed_count("a.service loaded active running Description"),
            None
        );
        for raw in [
            "",
            "header\n",
            "header\n1000 5 995 nope",
            "header\n1000 5 995 1%\n1000 5 995 1%",
            "header\n1000 5 995 1% extra",
        ] {
            assert_eq!(filesystem_usage(raw), None);
        }
        assert_eq!(filesystem_usage(DF), Some((1000, 250, 750, 25)));
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("ecosystem-test-{}", uuid::Uuid::new_v4()));
            fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn metadata_reads_only_line_count_and_modified_time() {
        let dir = TempDir::new();
        let path = dir.0.join(STATUS_FILE);
        assert!(status_metadata(&dir.0).is_none());
        fs::write(&path, "# Header\r\nfixture-private\nlast line").unwrap();
        let metadata = status_metadata(&dir.0).unwrap();
        assert_eq!(metadata.line_count, 3);
        assert_eq!(
            metadata.last_modified,
            DateTime::<Utc>::from(fs::metadata(&path).unwrap().modified().unwrap())
        );
        let brief = collect_with(|_| None, Some(metadata));
        assert!(!brief.to_string().contains("fixture-private"));
        fs::write(&path, "").unwrap();
        assert_eq!(status_metadata(&dir.0).unwrap().line_count, 0);
    }

    #[test]
    fn metadata_rejects_symlinks_oversized_and_invalid_utf8_files() {
        let dir = TempDir::new();
        let path = dir.0.join(STATUS_FILE);
        let other = dir.0.join("other.md");
        fs::write(&other, "fixture-private").unwrap();
        symlink(&other, &path).unwrap();
        assert!(status_metadata(&dir.0).is_none());
        fs::remove_file(&path).unwrap();
        fs::write(&path, vec![b'x'; MAX_STATUS_BYTES + 1]).unwrap();
        assert!(status_metadata(&dir.0).is_none());
        fs::write(&path, [0xff]).unwrap();
        assert!(status_metadata(&dir.0).is_none());
    }
}
