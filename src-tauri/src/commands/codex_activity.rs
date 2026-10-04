//! Read-only activity from the existing Codex service. A fresh app-server
//! process cannot observe the conversations already running in another one.

use serde::Serialize;

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct CodexActivity {
    pub working_conversations: u32,
    pub working_helpers: u32,
    pub waiting: u32,
    pub idle: u32,
    pub errored: u32,
}

#[tauri::command]
pub async fn get_codex_activity() -> Result<CodexActivity, String> {
    #[cfg(unix)]
    {
        let socket = crate::utils::codex_paths::codex_home()?
            .join("app-server-control")
            .join("app-server-control.sock");
        local::fetch(&socket, std::time::Duration::from_secs(3)).await
    }
    #[cfg(not(unix))]
    {
        Err("Live Codex activity is not available on this platform yet.".into())
    }
}

#[cfg(unix)]
mod local {
    use super::CodexActivity;
    use futures_util::{SinkExt, StreamExt};
    use serde::Deserialize;
    use serde_json::{json, Value};
    use std::collections::HashSet;
    use std::path::Path;
    use std::time::Duration;
    use tokio::net::UnixStream;
    use tokio_tungstenite::tungstenite::{protocol::WebSocketConfig, Message};
    use tokio_tungstenite::{client_async_with_config, WebSocketStream};

    const MAX_THREADS: usize = 512;
    const MAX_PAGES: usize = 32;
    const MAX_MESSAGE_BYTES: usize = 512 * 1024;
    const PROTOCOL_ERROR: &str = "Codex returned an unsupported live activity response.";

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LoadedThreads {
        data: Vec<String>,
        next_cursor: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ThreadMetadata {
        id: String,
        parent_thread_id: Option<String>,
        source: Option<Value>,
        status: ThreadStatus,
    }

    #[derive(Deserialize)]
    #[serde(tag = "type", rename_all = "camelCase")]
    enum ThreadStatus {
        Active {
            #[serde(rename = "activeFlags")]
            active_flags: Vec<ActiveFlag>,
        },
        Idle,
        NotLoaded,
        SystemError,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    enum ActiveFlag {
        WaitingOnApproval,
        WaitingOnUserInput,
    }

    fn count_thread(counts: &mut CodexActivity, thread: ThreadMetadata) {
        match thread.status {
            ThreadStatus::Active { active_flags } if !active_flags.is_empty() => {
                // A session waiting for both input and approval still counts once.
                counts.waiting += 1;
            }
            ThreadStatus::Active { .. } => {
                let helper = thread.parent_thread_id.is_some()
                    || thread
                        .source
                        .as_ref()
                        .and_then(|s| s.get("subAgent"))
                        .is_some();
                if helper {
                    counts.working_helpers += 1;
                } else {
                    counts.working_conversations += 1;
                }
            }
            ThreadStatus::Idle => counts.idle += 1,
            ThreadStatus::SystemError => counts.errored += 1,
            // A thread may unload between listing it and reading its metadata.
            ThreadStatus::NotLoaded => {}
        }
    }

    struct Client {
        socket: WebSocketStream<UnixStream>,
        next_id: u32,
    }

    impl Client {
        async fn send(&mut self, message: Value) -> Result<(), String> {
            self.socket
                .send(Message::Text(message.to_string().into()))
                .await
                .map_err(|_| "The connection to Codex closed. Try again.".to_string())
        }

        async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
            self.next_id += 1;
            let id = self.next_id;
            self.send(json!({"id": id, "method": method, "params": params}))
                .await?;
            while let Some(message) = self.socket.next().await {
                match message.map_err(|_| PROTOCOL_ERROR.to_string())? {
                    Message::Text(text) => {
                        let value: Value =
                            serde_json::from_str(&text).map_err(|_| PROTOCOL_ERROR.to_string())?;
                        if value.get("method").is_some() {
                            // We never subscribe, resume threads, or answer approvals.
                            if value["method"] == "currentTime/read" && value.get("id").is_some() {
                                self.send(json!({
                                    "id": value["id"],
                                    "result": {"currentTimeAt": chrono::Utc::now().timestamp()}
                                }))
                                .await?;
                            }
                            continue;
                        }
                        if value["id"].as_u64() != Some(u64::from(id)) {
                            continue;
                        }
                        if value.get("error").is_some() {
                            return Err("Codex could not provide live activity. Try again.".into());
                        }
                        return value
                            .get("result")
                            .cloned()
                            .ok_or_else(|| PROTOCOL_ERROR.into());
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) => {
                        self.socket
                            .flush()
                            .await
                            .map_err(|_| PROTOCOL_ERROR.to_string())?;
                    }
                    Message::Pong(_) => {}
                    _ => return Err(PROTOCOL_ERROR.into()),
                }
            }
            Err("The connection to Codex closed. Try again.".into())
        }

        async fn snapshot(&mut self) -> Result<CodexActivity, String> {
            let mut cursor = None;
            let mut cursors = HashSet::new();
            let mut ids = Vec::new();
            let mut seen_ids = HashSet::new();
            let mut complete = false;
            for _ in 0..MAX_PAGES {
                let page: LoadedThreads = serde_json::from_value(
                    self.request("thread/loaded/list", json!({"limit": 64, "cursor": cursor}))
                        .await?,
                )
                .map_err(|_| PROTOCOL_ERROR.to_string())?;
                for id in page.data {
                    if seen_ids.insert(id.clone()) {
                        ids.push(id);
                    }
                }
                if ids.len() > MAX_THREADS {
                    return Err("Too many loaded Codex sessions to read live activity.".into());
                }
                cursor = page.next_cursor;
                match &cursor {
                    None => {
                        complete = true;
                        break;
                    }
                    Some(next) if !cursors.insert(next.clone()) => {
                        return Err(PROTOCOL_ERROR.into())
                    }
                    _ => {}
                }
            }
            if !complete {
                return Err(PROTOCOL_ERROR.into());
            }

            let mut counts = CodexActivity::default();
            for id in ids {
                let response = self
                    .request(
                        "thread/read",
                        json!({
                            "threadId": id, "includeTurns": false
                        }),
                    )
                    .await?;
                let thread: ThreadMetadata = serde_json::from_value(response["thread"].clone())
                    .map_err(|_| PROTOCOL_ERROR.to_string())?;
                if thread.id != id {
                    return Err(PROTOCOL_ERROR.into());
                }
                count_thread(&mut counts, thread);
            }
            Ok(counts)
        }
    }

    pub(super) async fn fetch(path: &Path, timeout: Duration) -> Result<CodexActivity, String> {
        tokio::time::timeout(timeout, async {
            let stream = UnixStream::connect(path)
                .await
                .map_err(|_| "Open Codex to connect to its live activity.".to_string())?;
            let config = WebSocketConfig::default()
                .read_buffer_size(4096)
                .max_message_size(Some(MAX_MESSAGE_BYTES))
                .max_frame_size(Some(MAX_MESSAGE_BYTES));
            // The transport stays on this Unix socket; this URL only supplies
            // the local HTTP Upgrade handshake and does not make a TCP request.
            let (socket, _) = client_async_with_config("ws://localhost/", stream, Some(config))
                .await
                .map_err(|_| "Could not connect to Codex live activity.".to_string())?;
            let mut client = Client { socket, next_id: 0 };
            client
                .request(
                    "initialize",
                    json!({
                        "clientInfo": {
                            "name": "agentharbor_activity", "title": "AgentHarbor",
                            "version": env!("CARGO_PKG_VERSION")
                        },
                        "capabilities": {"experimentalApi": true}
                    }),
                )
                .await?;
            client.send(json!({"method": "initialized"})).await?;
            let result = client.snapshot().await;
            let _ = client.socket.close(None).await;
            result
        })
        .await
        .map_err(|_| "Codex live activity timed out. Try again.".to_string())?
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tokio::net::UnixListener;
        use tokio_tungstenite::accept_async;

        fn runtime() -> tokio::runtime::Runtime {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        }

        fn listener() -> (tempfile::TempDir, std::path::PathBuf, UnixListener) {
            // macOS limits Unix socket paths to 104 bytes.
            let directory = tempfile::Builder::new()
                .prefix("ah-live-")
                .tempdir_in("/tmp")
                .unwrap();
            let path = directory.path().join("codex.sock");
            let listener = UnixListener::bind(&path).unwrap();
            (directory, path, listener)
        }

        async fn serve(listener: &UnixListener, rows: Vec<Value>) -> usize {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let ids: Vec<Value> = rows.iter().map(|row| row["id"].clone()).collect();
            let mut reads = 0;
            while let Some(message) = socket.next().await {
                let message = message.unwrap();
                let Message::Text(text) = message else { break };
                let request: Value = serde_json::from_str(&text).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "initialize" => {
                        assert_eq!(request["params"]["capabilities"]["experimentalApi"], true);
                        json!({})
                    }
                    "initialized" => continue,
                    "thread/loaded/list" => {
                        assert_eq!(request["params"]["limit"], 64);
                        if request["params"]["cursor"].is_null() {
                            json!({"data": [ids[0]], "nextCursor": "page-2"})
                        } else {
                            assert_eq!(request["params"]["cursor"], "page-2");
                            // Repeat the first ID, as can happen during pagination.
                            json!({"data": ids, "nextCursor": null})
                        }
                    }
                    "thread/read" => {
                        assert_eq!(request["params"]["includeTurns"], false);
                        reads += 1;
                        let row = rows
                            .iter()
                            .find(|r| r["id"] == request["params"]["threadId"])
                            .unwrap();
                        json!({"thread": row})
                    }
                    unexpected => panic!("Activity client sent an unexpected method: {unexpected}"),
                };
                // Notifications and unrelated response IDs must not become counts.
                socket
                    .send(Message::Text(
                        json!({"method":"thread/status/changed","params":{}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                socket
                    .send(Message::Text(
                        json!({"id":999999,"result":{}}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                socket
                    .send(Message::Text(
                        json!({"id":request["id"],"result":result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
            reads
        }

        #[test]
        fn reads_all_pages_without_counting_idle_waiting_or_duplicate_threads_as_working() {
            runtime().block_on(async {
                let (_directory, path, listener) = listener();
                let rows = vec![
                    json!({"id":"root","status":{"type":"active","activeFlags":[]}}),
                    json!({"id":"helper","parentThreadId":"root","status":{"type":"active","activeFlags":[]}}),
                    json!({"id":"legacy-helper","source":{"subAgent":{}},"status":{"type":"active","activeFlags":[]}}),
                    json!({"id":"approval","status":{"type":"active","activeFlags":["waitingOnApproval"]}}),
                    json!({"id":"input","parentThreadId":"root","status":{"type":"active","activeFlags":["waitingOnUserInput","waitingOnApproval"]}}),
                    json!({"id":"idle","parentThreadId":"root","status":{"type":"idle"}}),
                    json!({"id":"error","status":{"type":"systemError"}}),
                    json!({"id":"unloaded","status":{"type":"notLoaded"}}),
                ];
                let server = tokio::spawn(async move { serve(&listener, rows).await });
                let activity = fetch(&path, Duration::from_secs(2)).await.unwrap();
                assert_eq!(activity, CodexActivity {
                    working_conversations: 1, working_helpers: 2, waiting: 2, idle: 1, errored: 1,
                });
                assert_eq!(server.await.unwrap(), 8);
            });
        }

        #[test]
        fn next_snapshot_observes_a_completed_turn() {
            runtime().block_on(async {
                let (_directory, path, listener) = listener();
                let server = tokio::spawn(async move {
                    serve(
                        &listener,
                        vec![json!({"id":"root","status":{"type":"active","activeFlags":[]}})],
                    )
                    .await;
                    serve(
                        &listener,
                        vec![json!({"id":"root","status":{"type":"idle"}})],
                    )
                    .await;
                });
                assert_eq!(
                    fetch(&path, Duration::from_secs(2))
                        .await
                        .unwrap()
                        .working_conversations,
                    1
                );
                let completed = fetch(&path, Duration::from_secs(2)).await.unwrap();
                assert_eq!(completed.working_conversations, 0);
                assert_eq!(completed.idle, 1);
                server.await.unwrap();
            });
        }

        #[test]
        fn unrecognized_status_is_unavailable_instead_of_zero() {
            runtime().block_on(async {
                let (_directory, path, listener) = listener();
                let server = tokio::spawn(async move {
                    serve(
                        &listener,
                        vec![json!({"id":"root","status":{"type":"futureStatus"}})],
                    )
                    .await;
                });
                assert!(fetch(&path, Duration::from_secs(2))
                    .await
                    .unwrap_err()
                    .contains("unsupported"));
                server.await.unwrap();
            });
        }

        #[test]
        fn unavailable_service_is_an_error_instead_of_zero() {
            runtime().block_on(async {
                let directory = tempfile::tempdir().unwrap();
                assert!(fetch(
                    &directory.path().join("missing.sock"),
                    Duration::from_secs(1)
                )
                .await
                .is_err());
            });
        }

        #[test]
        fn deadline_includes_the_websocket_handshake() {
            runtime().block_on(async {
                let (_directory, path, listener) = listener();
                let server = tokio::spawn(async move {
                    let (_stream, _) = listener.accept().await.unwrap();
                    std::future::pending::<()>().await;
                });
                let result = fetch(&path, Duration::from_millis(50)).await;
                assert!(result.unwrap_err().contains("timed out"));
                server.abort();
            });
        }
    }
}
