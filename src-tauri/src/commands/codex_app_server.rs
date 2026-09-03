//! Minimal client for the documented `codex app-server` stdio protocol.
//!
//! App Server speaks newline-delimited JSON-RPC-like messages without the
//! `jsonrpc` field. A fresh child is used for each request so AgentHarbor does
//! not keep an authenticated Codex process alive after a window closes.

use serde_json::{json, Value};
use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MESSAGE_QUEUE_CAPACITY: usize = 8;
const MAX_SERVER_REQUESTS: usize = 16;

#[derive(Debug)]
enum ReaderEvent {
    Message(Value),
    Protocol(String),
}

fn parsed_node_version(name: &str) -> Option<Vec<u64>> {
    let version = name.strip_prefix('v').unwrap_or(name);
    if version.is_empty() {
        return None;
    }
    version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

fn compare_node_version_names(left: &str, right: &str) -> Ordering {
    match (parsed_node_version(left), parsed_node_version(right)) {
        (Some(left), Some(right)) => left.cmp(&right).then_with(|| left.len().cmp(&right.len())),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => left.cmp(right),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppServerError {
    Unavailable(String),
    Timeout,
    Protocol(String),
    Remote { code: Option<i64>, message: String },
}

impl AppServerError {
    /// File fallback is allowed only when App Server cannot be used, not when
    /// it rejected a valid request because of policy or validation.
    pub fn permits_file_fallback(&self) -> bool {
        matches!(
            self,
            Self::Unavailable(_) | Self::Timeout | Self::Protocol(_)
        ) || matches!(
            self,
            Self::Remote {
                code: Some(-32601),
                ..
            }
        )
    }
}

impl fmt::Display for AppServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => write!(f, "Codex App Server unavailable: {message}"),
            Self::Timeout => write!(f, "Codex App Server timed out"),
            Self::Protocol(message) => write!(f, "Codex App Server protocol error: {message}"),
            Self::Remote { code, message } => match code {
                Some(code) => write!(f, "Codex App Server error {code}: {message}"),
                None => write!(f, "Codex App Server error: {message}"),
            },
        }
    }
}

fn resolve_codex_program() -> Result<PathBuf, AppServerError> {
    if let Ok(path) = which::which("codex") {
        return Ok(path);
    }

    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
    ];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local").join("bin").join("codex"));
        candidates.push(home.join(".npm-global").join("bin").join("codex"));
        candidates.push(home.join(".volta").join("bin").join("codex"));

        // GUI-launched macOS apps usually do not inherit the shell setup that
        // adds nvm's active Node bin directory to PATH. Check every installed
        // nvm version and prefer the newest parsed Node version.
        let nvm_versions = home.join(".nvm").join("versions").join("node");
        if let Ok(entries) = fs::read_dir(nvm_versions) {
            let mut nvm_candidates: Vec<(String, PathBuf)> = entries
                .flatten()
                .filter_map(|entry| {
                    let version = entry.file_name().to_string_lossy().to_string();
                    let path = entry.path().join("bin").join("codex");
                    path.is_file().then_some((version, path))
                })
                .collect();
            nvm_candidates.sort_by(|left, right| {
                compare_node_version_names(&right.0, &left.0).then_with(|| right.0.cmp(&left.0))
            });
            candidates.extend(nvm_candidates.into_iter().map(|(_, path)| path));
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| AppServerError::Unavailable("the `codex` executable was not found".into()))
}

fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    {
        // The child starts its own process group below. Kill that group so an
        // npm or nvm launcher cannot leave a Node descendant holding stdout.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn read_bounded_frame(reader: &mut impl BufRead, max_bytes: usize) -> io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Ok(Some(frame))
            };
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.unwrap_or(available.len());
        if frame.len().saturating_add(take) > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("App Server frame exceeds {max_bytes} bytes"),
            ));
        }
        frame.extend_from_slice(&available[..take]);
        reader.consume(take + usize::from(newline.is_some()));

        if newline.is_some() {
            if frame.last() == Some(&b'\r') {
                frame.pop();
            }
            return Ok(Some(frame));
        }
    }
}

fn response_for_id(value: Value, expected_id: i64) -> Option<Result<Value, AppServerError>> {
    if value.get("id").and_then(Value::as_i64) != Some(expected_id) {
        return None;
    }
    if let Some(error) = value.get("error") {
        let code = error.get("code").and_then(Value::as_i64);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown App Server error")
            .to_string();
        return Some(Err(AppServerError::Remote { code, message }));
    }
    Some(
        value.get("result").cloned().ok_or_else(|| {
            AppServerError::Protocol("response has neither result nor error".into())
        }),
    )
}

fn response_to_server_request(value: &Value) -> Result<Option<Value>, AppServerError> {
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(id) = value.get("id") else {
        return Ok(None);
    };
    if !id.is_string() && id.as_i64().is_none() {
        return Err(AppServerError::Protocol(format!(
            "server request '{method}' has an invalid id"
        )));
    }
    let id = id.clone();

    Ok(Some(if method == "currentTime/read" {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .min(i64::MAX as u64) as i64;
        json!({ "id": id, "result": { "currentTimeAt": current_time } })
    } else {
        json!({
            "id": id,
            "error": {
                "code": -32601,
                "message": "AgentHarbor does not support this App Server request"
            }
        })
    }))
}

fn receive_response(
    receiver: &mpsc::Receiver<ReaderEvent>,
    stdin: &mut impl Write,
    expected_id: i64,
    deadline: Instant,
) -> Result<Value, AppServerError> {
    let mut server_requests = 0usize;
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(AppServerError::Timeout)?;
        match receiver.recv_timeout(remaining) {
            Ok(ReaderEvent::Message(value)) => {
                if let Some(response) = response_to_server_request(&value)? {
                    server_requests += 1;
                    if server_requests > MAX_SERVER_REQUESTS {
                        return Err(AppServerError::Protocol(
                            "App Server sent too many server requests".into(),
                        ));
                    }
                    write_message(stdin, &response)?;
                    continue;
                }
                if let Some(result) = response_for_id(value, expected_id) {
                    return result;
                }
            }
            Ok(ReaderEvent::Protocol(message)) => {
                return Err(AppServerError::Protocol(message));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return Err(AppServerError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(AppServerError::Protocol(
                    "process closed stdout before replying".into(),
                ));
            }
        }
    }
}

fn write_message(stdin: &mut impl Write, value: &Value) -> Result<(), AppServerError> {
    serde_json::to_writer(&mut *stdin, value)
        .map_err(|error| AppServerError::Protocol(error.to_string()))?;
    stdin
        .write_all(b"\n")
        .and_then(|_| stdin.flush())
        .map_err(|error| AppServerError::Unavailable(error.to_string()))
}

fn request_with_program(
    program: &Path,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, AppServerError> {
    let mut command = Command::new(program);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    if let Some(program_directory) = program.parent() {
        let mut search_paths = vec![program_directory.to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            search_paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(path) = std::env::join_paths(search_paths) {
            // npm and nvm Codex launchers use `/usr/bin/env node`. GUI apps do
            // not always inherit the shell PATH, so include the launcher's
            // directory where its matching Node binary is normally located.
            command.env("PATH", path);
        }
    }

    let mut child = command
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| AppServerError::Unavailable(error.to_string()))?;

    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            stop_child(&mut child);
            return Err(AppServerError::Protocol(
                "stdin pipe was not created".into(),
            ));
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            stop_child(&mut child);
            return Err(AppServerError::Protocol(
                "stdout pipe was not created".into(),
            ));
        }
    };

    let (sender, receiver) = mpsc::sync_channel(MESSAGE_QUEUE_CAPACITY);
    let reader = std::thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        loop {
            match read_bounded_frame(&mut stdout, MAX_FRAME_BYTES) {
                Ok(Some(frame)) => {
                    let Ok(value) = serde_json::from_slice::<Value>(&frame) else {
                        continue;
                    };
                    if value.get("id").is_none() {
                        continue;
                    }
                    if sender.send(ReaderEvent::Message(value)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = sender.send(ReaderEvent::Protocol(error.to_string()));
                    break;
                }
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let initialize = json!({
        "method": "initialize",
        "id": 1,
        "params": {
            "clientInfo": {
                "name": "agentharbor",
                "title": "AgentHarbor",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "experimentalApi": true,
                "requestAttestation": false
            }
        }
    });

    let result = (|| {
        write_message(&mut stdin, &initialize)?;
        receive_response(&receiver, &mut stdin, 1, deadline)?;
        write_message(&mut stdin, &json!({ "method": "initialized" }))?;
        write_message(
            &mut stdin,
            &json!({ "method": method, "id": 2, "params": params }),
        )?;
        receive_response(&receiver, &mut stdin, 2, deadline)
    })();

    drop(stdin);
    drop(receiver);
    stop_child(&mut child);
    // Do not let a misbehaving descendant extend the public request timeout.
    // The process-group kill normally closes stdout immediately. If it does
    // not, dropping the handle safely detaches the reader instead of hanging.
    if reader.is_finished() {
        let _ = reader.join();
    }
    result
}

pub fn request(method: &str, params: Value) -> Result<Value, AppServerError> {
    let program = resolve_codex_program()?;
    request_with_program(&program, method, params, DEFAULT_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_success_response_by_id() {
        let value = json!({ "id": 2, "result": { "ok": true } });
        assert_eq!(
            response_for_id(value, 2).unwrap().unwrap(),
            json!({ "ok": true })
        );
    }

    #[test]
    fn creates_current_time_response_for_server_request() {
        let response = response_to_server_request(&json!({
            "id": "clock-1",
            "method": "currentTime/read",
            "params": {}
        }))
        .unwrap()
        .unwrap();
        assert_eq!(response["id"], json!("clock-1"));
        assert!(response["result"]["currentTimeAt"].as_i64().is_some());
    }

    #[test]
    fn rejects_unsupported_server_request() {
        let response = response_to_server_request(&json!({
            "id": 9,
            "method": "account/chatgptAuthTokens/refresh",
            "params": {}
        }))
        .unwrap()
        .unwrap();
        assert_eq!(response["error"]["code"], json!(-32601));
    }

    #[test]
    fn rejects_invalid_server_request_id() {
        let error = response_to_server_request(&json!({
            "id": { "unexpected": true },
            "method": "currentTime/read",
            "params": {}
        }))
        .unwrap_err();
        assert!(matches!(error, AppServerError::Protocol(_)));
    }

    #[test]
    fn bounds_individual_stdout_frames() {
        let mut accepted = BufReader::new(&b"1234\n"[..]);
        assert_eq!(
            read_bounded_frame(&mut accepted, 4).unwrap(),
            Some(b"1234".to_vec())
        );

        let mut rejected = BufReader::new(&b"12345\n"[..]);
        assert_eq!(
            read_bounded_frame(&mut rejected, 4).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn orders_nvm_versions_numerically() {
        let mut versions = ["v9.11.2", "v20.2.0", "v20.10.0", "not-a-version"];
        versions.sort_by(|left, right| compare_node_version_names(right, left));
        assert_eq!(
            versions,
            ["v20.10.0", "v20.2.0", "v9.11.2", "not-a-version"]
        );
    }

    #[test]
    fn preserves_remote_error_code() {
        let value = json!({ "id": 2, "error": { "code": -32601, "message": "missing" } });
        let error = response_for_id(value, 2).unwrap().unwrap_err();
        assert!(error.permits_file_fallback());
        assert_eq!(
            error,
            AppServerError::Remote {
                code: Some(-32601),
                message: "missing".into()
            }
        );
    }

    #[test]
    fn policy_error_does_not_permit_file_fallback() {
        let error = AppServerError::Remote {
            code: Some(-32000),
            message: "managed policy rejected write".into(),
        };
        assert!(!error.permits_file_fallback());
    }

    #[test]
    fn missing_program_is_reported_without_hanging() {
        let error = request_with_program(
            Path::new("/definitely/not/a/codex-binary"),
            "config/read",
            json!({}),
            Duration::from_millis(20),
        )
        .unwrap_err();
        assert!(matches!(error, AppServerError::Unavailable(_)));
    }

    #[cfg(unix)]
    #[test]
    fn unresponsive_child_is_killed_at_timeout() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-codex");
        std::fs::write(&script, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();

        let started = Instant::now();
        let error =
            request_with_program(&script, "config/read", json!({}), Duration::from_millis(50))
                .unwrap_err();
        assert_eq!(error, AppServerError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
