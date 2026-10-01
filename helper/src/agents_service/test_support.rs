//! Synthetic fixtures for agent-service tests: a private temporary home with
//! XDG directories, and valid personas. Nothing here touches the real home.
use super::store::{Paths, Persona};
use futures_util::{SinkExt, StreamExt};
use nostr::Event;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::{
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

pub const ROOM_A: &str = "00000000-0000-4000-8000-0000000000b1";
pub const ROOM_B: &str = "00000000-0000-4000-8000-0000000000b2";
pub const ROOM_C: &str = "00000000-0000-4000-8000-0000000000b3";
/// The community of `persona`'s default record.
pub const RELAY: &str = "wss://relay.example/";

pub struct TempHome {
    pub base: PathBuf,
    pub paths: Paths,
}
impl TempHome {
    pub fn new() -> Self {
        let base =
            std::env::temp_dir().join(format!("omarchy-buzz-agents-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&base)
            .unwrap();
        let home = base.join("home");
        let paths = Paths {
            state: home.join(".local/state"),
            config: home.join(".config"),
            data: home.join(".local/share"),
            home: home.clone(),
        };
        for dir in [&home, &paths.state, &paths.config, &paths.data] {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .unwrap();
        }
        Self { base, paths }
    }
    /// A private (0700) directory under the home, as a user-chosen workspace.
    pub fn private_dir(&self, relative: &str) -> String {
        let path = self.paths.home.join(relative);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)
            .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path.to_str().unwrap().to_owned()
    }
}
impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

pub fn persona(id: &str, workspace: &str) -> Persona {
    Persona {
        id: id.into(),
        name: "Scout".into(),
        description: "Reads the logs.".into(),
        instructions: "Answer briefly.\nCite files.".into(),
        harness: "codex".into(),
        model: String::new(),
        acp_command: "buzz-acp".into(),
        rooms: vec![ROOM_A.into()],
        respond_to: "owner-only".into(),
        workspace: workspace.into(),
        identity: None,
        start_at_login: false,
        answers_dms: false,
        relay: RELAY.into(),
        auth_tag: None,
        published: false,
        member_rooms: Vec::new(),
        published_at: 0,
        last_error: None,
        primary: true,
    }
}

pub fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).unwrap().mode() & 0o7777
}

/// A synthetic loopback relay: NIP-42 AUTH for any key, then one scripted
/// answer per EVENT. Every event is verified and recorded with its connection.
#[derive(Clone, Copy, PartialEq)]
pub enum Answer {
    Accept,
    Reject,
    Silent,
    Close,
}

#[derive(Clone, Debug)]
pub struct Seen {
    /// Connection index and its authenticated key.
    pub connection: usize,
    pub author: nostr::PublicKey,
    /// Tags of that connection's AUTH event.
    pub auth_tags: Vec<Vec<String>>,
    pub event: Event,
}

pub struct Relay {
    pub url: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// `answer(kind)` decides the reply to each EVENT; `reject_auth` refuses AUTH.
pub async fn relay(answer: fn(u16) -> Answer, reject_auth: bool) -> Relay {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (record, relay_url) = (seen.clone(), url.clone());
    let task = tokio::spawn(async move {
        let mut index = 0;
        loop {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            let challenge = format!("challenge-{index}");
            ws.send(Message::Text(json!(["AUTH", challenge]).to_string().into()))
                .await
                .unwrap();
            let Some(Ok(Message::Text(text))) = ws.next().await else {
                continue;
            };
            let frame: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(frame[0], "AUTH");
            let auth: Event = serde_json::from_value(frame[1].clone()).unwrap();
            auth.verify().unwrap();
            assert!(auth
                .tags
                .iter()
                .any(|t| t.as_slice() == ["relay", relay_url.as_str()]));
            ws.send(Message::Text(
                json!([
                    "OK",
                    auth.id.to_hex(),
                    !reject_auth,
                    if reject_auth {
                        "restricted: synthetic-sensitive-reason"
                    } else {
                        ""
                    }
                ])
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
            let auth_tags: Vec<Vec<String>> =
                auth.tags.iter().map(|t| t.as_slice().to_vec()).collect();
            while let Some(Ok(message)) = ws.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(frame[0], "EVENT", "only EVENT frames are expected: {frame}");
                let event: Event = serde_json::from_value(frame[1].clone()).unwrap();
                event.verify().unwrap();
                record.lock().unwrap().push(Seen {
                    connection: index,
                    author: auth.pubkey,
                    auth_tags: auth_tags.clone(),
                    event: event.clone(),
                });
                let id = event.id.to_hex();
                match answer(event.kind.as_u16()) {
                    Answer::Accept => ws
                        .send(Message::Text(
                            json!(["OK", id, true, ""]).to_string().into(),
                        ))
                        .await
                        .unwrap(),
                    Answer::Reject => ws
                        .send(Message::Text(
                            json!(["OK", id, false, "blocked: synthetic-sensitive-reason"])
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap(),
                    Answer::Silent => {}
                    Answer::Close => break,
                }
            }
            index += 1;
        }
    });
    Relay { url, seen, task }
}
