use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use crate::notification::{parse_notify_line, NotificationEvent, MAX_LINE_BYTES};

#[derive(Debug)]
pub enum BindOutcome {
    Bound(UnixListener),
    AlreadyRunning,
    Failed(std::io::Error),
}

pub fn control_socket_path() -> Result<PathBuf, crate::settings::SettingsError> {
    Ok(crate::settings::app_support_dir()?.join("control.sock"))
}

fn set_socket_perms(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        let _ = std::fs::set_permissions(path, perms);
    }
}

/// Bind the control socket. On a pre-existing path, probe whether a live instance
/// holds it (-> `AlreadyRunning`) or it is a stale file (-> unlink + rebind).
/// Returns an outcome rather than exiting, so tests never call `process::exit`.
pub fn bind_control_socket(path: &Path) -> BindOutcome {
    match UnixListener::bind(path) {
        Ok(listener) => {
            set_socket_perms(path);
            BindOutcome::Bound(listener)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => match UnixStream::connect(path) {
            Ok(_) => BindOutcome::AlreadyRunning,
            Err(_) => {
                let _ = std::fs::remove_file(path);
                match UnixListener::bind(path) {
                    Ok(listener) => {
                        set_socket_perms(path);
                        BindOutcome::Bound(listener)
                    }
                    Err(e) => BindOutcome::Failed(e),
                }
            }
        },
        Err(e) => BindOutcome::Failed(e),
    }
}

/// Read + parse a single bounded event line from a connection.
pub fn read_event(stream: UnixStream) -> Result<NotificationEvent, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = reader
        .by_ref()
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_line(&mut line)?;
    if n > MAX_LINE_BYTES {
        return Err(Box::new(crate::notification::NotifyParseError::TooLong));
    }
    Ok(parse_notify_line(line.trim_end())?)
}

/// Client: connect and write one JSON event line.
pub fn send_notify(path: &Path, event: &NotificationEvent) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    let mut json = serde_json::to_string(event).expect("NotificationEvent serializes");
    json.push('\n');
    stream.write_all(json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn binds_clean_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("control.sock");
        match bind_control_socket(&path) {
            BindOutcome::Bound(_listener) => {}
            other => panic!("expected Bound, got {other:?}"),
        }
    }

    #[test]
    fn detects_already_running() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let _live = match bind_control_socket(&path) {
            BindOutcome::Bound(l) => l, // keep the listener alive
            other => panic!("expected Bound, got {other:?}"),
        };
        match bind_control_socket(&path) {
            BindOutcome::AlreadyRunning => {}
            other => panic!("expected AlreadyRunning, got {other:?}"),
        }
    }

    #[test]
    fn rebinds_over_stale_socket() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("control.sock");
        drop(match bind_control_socket(&path) {
            BindOutcome::Bound(l) => l,
            other => panic!("expected Bound, got {other:?}"),
        });
        match bind_control_socket(&path) {
            BindOutcome::Bound(_) => {}
            other => panic!("expected Bound after stale cleanup, got {other:?}"),
        }
    }

    #[test]
    fn read_event_rejects_overlong_padded_line() {
        use std::io::Write;
        use std::os::unix::net::UnixStream;
        let (mut writer, reader) = UnixStream::pair().unwrap();
        // Short valid JSON padded well past MAX_LINE_BYTES with trailing spaces + newline.
        let payload = format!(
            "{}{}\n",
            r#"{"kind":"running"}"#,
            " ".repeat(MAX_LINE_BYTES)
        );
        // Write on a thread (payload exceeds the socket buffer and would block).
        let h = std::thread::spawn(move || {
            let _ = writer.write_all(payload.as_bytes());
        });
        assert!(
            read_event(reader).is_err(),
            "overlong line must be rejected, not silently truncated+accepted"
        );
        let _ = h.join();
    }

    #[test]
    fn roundtrips_event_through_send_and_parse() {
        use crate::notification::NotificationEvent;
        let dir = tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = match bind_control_socket(&path) {
            BindOutcome::Bound(l) => l,
            other => panic!("{other:?}"),
        };
        let ev = NotificationEvent {
            kind: "running".into(),
            animation_name: None,
            label: Some("hi".into()),
            body: None,
            ttl_ms: Some(1000),
            priority: None,
        };
        let send_path = path.clone();
        let ev_clone = ev.clone();
        let sender = std::thread::spawn(move || send_notify(&send_path, &ev_clone).unwrap());
        let (stream, _) = listener.accept().unwrap();
        let got = read_event(stream).unwrap();
        sender.join().unwrap();
        assert_eq!(got, ev);
    }
}
