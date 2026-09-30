use crate::{
    config,
    protocol::{self, Status},
};
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::{mpsc, watch},
    time::{timeout, Duration},
};
const IO_DEADLINE: Duration = Duration::from_secs(10);
pub fn socket_path() -> Result<PathBuf, &'static str> {
    runtime_socket("control.sock")
}
/// `$XDG_RUNTIME_DIR/omarchy-buzz/<name>` after checking the runtime directory.
pub(crate) fn runtime_socket(name: &str) -> Result<PathBuf, &'static str> {
    let d = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("runtime_unavailable")?);
    let m = std::fs::symlink_metadata(&d).map_err(|_| "runtime_unavailable")?;
    if !d.is_absolute()
        || !m.is_dir()
        || m.uid() != rustix::process::getuid().as_raw()
        || m.mode() & 0o077 != 0
    {
        return Err("runtime_insecure");
    }
    Ok(d.join("omarchy-buzz").join(name))
}
pub(crate) fn private_dir(path: &std::path::Path) -> Result<(), &'static str> {
    let parent = path.parent().ok_or("runtime_unavailable")?;
    match std::fs::create_dir(parent) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("runtime_unavailable"),
    }
    let m = std::fs::symlink_metadata(parent).map_err(|_| "runtime_unavailable")?;
    if !m.is_dir() || m.uid() != rustix::process::getuid().as_raw() {
        return Err("runtime_insecure");
    }
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "runtime_unavailable")
}
pub(crate) fn socket_permissions(path: &std::path::Path) -> Result<(), &'static str> {
    use std::os::unix::fs::FileTypeExt;
    let m = std::fs::symlink_metadata(path).map_err(|_| "runtime_unavailable")?;
    if !m.file_type().is_socket()
        || m.uid() != rustix::process::getuid().as_raw()
        || m.mode() & 0o077 != 0
    {
        return Err("runtime_insecure");
    }
    Ok(())
}
pub(crate) async fn write<W: tokio::io::AsyncWrite + Unpin>(
    w: &mut W,
    v: &serde_json::Value,
) -> Result<(), &'static str> {
    let mut b = serde_json::to_vec(v).map_err(|_| "invalid_response")?;
    if b.len() + 1 > protocol::RESPONSE_LIMIT {
        return Err("oversized_response");
    }
    b.push(b'\n');
    timeout(IO_DEADLINE, w.write_all(&b))
        .await
        .map_err(|_| "io_timeout")?
        .map_err(|_| "io_unavailable")
}
// A lost actor reply is an unknown outcome (`unknown`), never a refusal.
async fn send_result(
    reply: tokio::sync::oneshot::Receiver<Option<&'static str>>,
    deadline: Duration,
    unknown: &'static str,
) -> Option<&'static str> {
    match tokio::time::timeout(deadline, reply).await {
        Ok(Ok(category)) => category,
        _ => Some(unknown),
    }
}
async fn client(
    s: UnixStream,
    mut status: watch::Receiver<Status>,
    retry: mpsc::Sender<protocol::Command>,
    instance: String,
) -> Result<(), &'static str> {
    peer_allowed(&s)?;
    let (read, mut out) = s.into_split();
    let mut read = BufReader::new(read);
    let snapshot = status.borrow().clone();
    write(
        &mut out,
        &protocol::envelope("hello", None, &instance, &snapshot),
    )
    .await?;
    let mut subscribed = false;
    let mut partial_frame = Vec::new();
    loop {
        tokio::select! {
            line=protocol::read_line_buffered(&mut read, &mut partial_frame)=> {
                let line=match line? { Some(l)=>l,None=>return Ok(()) };
                let r=match protocol::request(&line) { Ok(r)=>r,Err(category)=> { write(&mut out,&serde_json::json!({"version":1,"type":"error","category":category,"instanceId":instance})).await?; return Ok(()); } };
                if r.kind=="send_message" && (r.instance_id.as_deref()!=Some(instance.as_str()) || r.generation!=Some(status.borrow().generation)) {
                    write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":"send_scope_changed","instanceId":instance})).await?;
                    continue;
                }
                if r.kind=="open_dm" {
                    let refused={let current=status.borrow();
                        if r.instance_id.as_deref()!=Some(instance.as_str()) || r.generation!=Some(current.generation) {Some("dm_open_scope_changed")}
                        else if current.dm_open.state=="sending" && current.dm_open.request_id.as_deref()!=Some(r.id.as_str()) {Some("dm_open_busy")}
                        else {None}};
                    if let Some(category)=refused {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":category,"instanceId":instance})).await?;continue;}
                }
                if r.kind=="set_relay" || r.kind=="create_identity" {
                    let refused={let current=status.borrow();
                        // Only a helper that is not authenticated takes setup, so a
                        // working session is never replaced by accident.
                        if !matches!(current.connection.as_str(),"unconfigured"|"disconnected"|"unavailable") {
                            Some(if current.connection=="connecting" {"setup_busy"} else {"setup_not_allowed"})
                        } else if current.category.as_deref()==Some("identity_access_pending") {Some("setup_busy")}
                        else if r.kind=="set_relay" && config::canonical_relay(r.url.as_deref().unwrap_or_default()).is_err() {Some("setup_invalid_relay")}
                        else {None}};
                    if let Some(category)=refused {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":category,"instanceId":instance})).await?;continue;}
                }
                if r.kind=="claim_invite" || r.kind=="accept_invite" {
                    let refused={let current=status.borrow();
                        // Invites need a relay and an identity. A disconnected helper still
                        // holds the identity of its refused connection: a new identity may
                        // not authenticate before it is a member (handlers/auth.rs).
                        if current.relay.is_none() || current.identity.is_none() {Some("setup_not_allowed")}
                        else if matches!(current.setup.state.as_str(),"checking"|"claiming") {Some("setup_busy")}
                        else if matches!(current.connection.as_str(),"authenticated"|"disconnected") {None}
                        else if current.connection=="connecting" {Some("setup_busy")}
                        else {Some("setup_not_allowed")}};
                    if let Some(category)=refused {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":category,"instanceId":instance})).await?;continue;}
                }
                if r.kind=="mint_invite" {
                    // Minting needs a working session: the relay checks the owner or admin role.
                    let refused={let current=status.borrow();
                        if current.invites.state=="minting" {Some("setup_busy")}
                        else if current.connection!="authenticated" {Some("relay_unavailable")}
                        else {None}};
                    if let Some(category)=refused {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":category,"instanceId":instance})).await?;continue;}
                }
                if matches!(r.kind.as_str(),"download_attachment"|"upload_attachment"|"thumbnail_attachment") {
                    // Transfers need a working session; one download and one upload at a time.
                    let refused={let current=status.borrow();
                        if r.kind=="download_attachment" && current.download.state=="downloading" {Some("setup_busy")}
                        else if r.kind=="upload_attachment" && current.upload.state=="uploading" {Some("setup_busy")}
                        else if current.connection!="authenticated" {Some("relay_unavailable")}
                        else {None}};
                    if let Some(category)=refused {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":category,"instanceId":instance})).await?;continue;}
                }
                if r.kind=="join_room" || r.kind=="leave_room" {
                    let busy={let current=status.borrow();current.room_action.state=="sending" && current.room_action.request_id.as_deref()!=Some(r.id.as_str())};
                    if busy {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":"setup_busy","instanceId":instance})).await?;continue;}
                }
                if r.kind=="send_message" {
                    let busy={let pending=status.borrow();pending.delivery.state=="sending" && pending.delivery.request_id.as_deref()!=Some(r.id.as_str())};
                    if busy {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":"send_busy","instanceId":instance})).await?;continue;}
                }
                let mut send_reply=None;
                let setup=matches!(r.kind.as_str(),"set_relay"|"create_identity"|"claim_invite"|"accept_invite"|"mint_invite");
                let room_action=r.kind=="join_room" || r.kind=="leave_room";
                let media=matches!(r.kind.as_str(),"download_attachment"|"thumbnail_attachment"|"open_download"|"upload_attachment"|"remove_pending_attachment");
                // A setup reply that never arrives is not a refusal; the next
                // status frame shows whether the change was saved.
                let unknown=if r.kind=="open_dm" {"dm_open_unknown"} else if setup || room_action || media {"setup_busy"} else {"delivery_unknown"};
                let command=match r.kind.as_str() {
                    "retry_connection"=>Some(protocol::Command::Retry),
                    "fetch_recent"=>Some(protocol::Command::FetchRecent(r.room_id.clone().unwrap())),
                    "fetch_older"=>Some(protocol::Command::FetchOlder(r.room_id.clone().unwrap())),
                    "fetch_thread"=>Some(protocol::Command::FetchThread(r.room_id.clone().unwrap(),r.root_id.clone().unwrap())),
                    "close_thread"=>Some(protocol::Command::CloseThread),
                    "fetch_recipients"=>Some(protocol::Command::FetchRecipients(r.room_id.clone().unwrap())),
                    "send_message"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::SendChecked(protocol::SendIntent {
                        request_id:r.id.clone(),room:r.room_id.clone().unwrap(),root_id:r.root_id.clone(),text:r.text.clone().unwrap(),
                        mentions:r.mentions.clone().unwrap(),generation:r.generation.unwrap(),
                    },reply))},
                    "open_dm"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::OpenDm(protocol::DmOpenIntent {
                        request_id:r.id.clone(),participants:r.participants.clone().unwrap(),generation:r.generation.unwrap(),
                    },reply))},
                    "set_relay"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::SetRelay(r.url.clone().unwrap(),reply))},
                    "create_identity"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::CreateIdentity(reply))},
                    "claim_invite"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::ClaimInvite(r.input.clone().unwrap(),reply))},
                    "accept_invite"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::AcceptInvite(r.code.clone().unwrap(),r.policy_version.clone(),reply))},
                    "open_rooms"=>Some(protocol::Command::FetchOpenRooms),
                    "mint_invite"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::MintInvite(r.max_uses.unwrap(),r.expires_in_hours.unwrap(),reply))},
                    "join_room"|"leave_room"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);
                        let action=if r.kind=="join_room" {crate::join::Action::Join} else {crate::join::Action::Leave};
                        Some(protocol::Command::RoomAction(action,r.id.clone(),r.room_id.clone().unwrap(),reply))},
                    "download_attachment"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::DownloadAttachment(r.event_id.clone().unwrap(),r.hash.clone().unwrap(),reply))},
                    "thumbnail_attachment"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::ThumbnailAttachment(r.event_id.clone().unwrap(),r.hash.clone().unwrap(),reply))},
                    "open_download"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::OpenDownload(r.path.clone().unwrap(),reply))},
                    "upload_attachment"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::UploadAttachment(r.room_id.clone().unwrap(),r.root_id.clone(),r.path.clone().unwrap(),reply))},
                    "remove_pending_attachment"=>{let (reply,receiver)=tokio::sync::oneshot::channel();send_reply=Some(receiver);Some(protocol::Command::RemovePendingAttachment(r.hash.clone().unwrap(),reply))},
                    _=>None,
                };
                if let Some(command)=command {if retry.try_send(command).is_err() {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":if setup || room_action {"setup_busy"} else if media {"setup_busy"} else {"request_busy"},"instanceId":instance})).await?;continue;}}
                if let Some(reply)=send_reply {
                    // Relay discovery (up to 13 s), a Secret Service write and an invite's
                    // three HTTP requests (10 s each) take longer than a send.
                    let category=send_result(reply,Duration::from_secs(if setup {60} else if media {10} else {5}),unknown).await;
                    if let Some(category)=category {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":category,"instanceId":instance})).await?;continue;}
                }
                if r.kind=="subscribe" { subscribed=true; }
                let snapshot=status.borrow().clone();
                write(&mut out,&protocol::envelope("status",Some(&r.id),&instance,&snapshot)).await?;
            },
            changed=status.changed(), if subscribed=> { changed.map_err(|_|"daemon_unavailable")?; let snapshot=status.borrow_and_update().clone(); write(&mut out,&protocol::envelope("status",None,&instance,&snapshot)).await?; }
        }
    }
}
/// Only the same user may talk to a daemon, in either direction.
pub(crate) fn peer_allowed(s: &UnixStream) -> Result<(), &'static str> {
    if s.peer_cred().map_err(|_| "peer_unavailable")?.uid() != rustix::process::getuid().as_raw() {
        return Err("peer_denied");
    }
    Ok(())
}
/// A per-process instance identifier for status envelopes.
pub(crate) fn instance_id() -> Result<String, &'static str> {
    Ok(format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "clock_unavailable")?
            .as_nanos()
    ))
}
/// The listening socket: inherited from socket activation (checked against
/// `path`) or bound here. The boolean is true when bound here (standalone).
pub(crate) fn listen(path: &std::path::Path) -> Result<(UnixListener, bool), &'static str> {
    private_dir(path)?;
    let mut inherited = listenfd::ListenFd::from_env();
    let inherited_listener = inherited
        .take_unix_listener(0)
        .map_err(|_| "activation_invalid")?;
    let standalone = inherited_listener.is_none();
    let listener = if let Some(l) = inherited_listener {
        if l.local_addr()
            .map_err(|_| "activation_invalid")?
            .as_pathname()
            != Some(path)
        {
            return Err("activation_invalid");
        }
        socket_permissions(path)?;
        l.set_nonblocking(true).map_err(|_| "activation_invalid")?;
        UnixListener::from_std(l).map_err(|_| "activation_invalid")?
    } else {
        // Never unlink an existing socket: its daemon may still be alive.
        let l = UnixListener::bind(path).map_err(|_| "daemon_already_running")?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "runtime_unavailable")?;
        l
    };
    Ok((listener, standalone))
}
pub async fn daemon(keep: bool) -> Result<(), &'static str> {
    let c = config::load()?;
    let path = socket_path()?;
    let (listener, standalone) = listen(&path)?;
    let (tx, rx) = watch::channel(Status::new(&c));
    let (retry_tx, retry_rx) = mpsc::channel(1);
    let auth = tokio::spawn(crate::auth::run(c, tx, retry_rx));
    let count = Arc::new(AtomicUsize::new(0));
    let instance = instance_id()?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "signal_unavailable")?;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut idle = std::time::Instant::now();
    loop {
        tokio::select! {
            _=terminate.recv()=>break,
            signal=tokio::signal::ctrl_c()=>{signal.map_err(|_|"signal_unavailable")?;break;},
            _=tick.tick()=> { if count.load(Ordering::SeqCst)>0 {idle=std::time::Instant::now();} else if !keep && idle.elapsed()>Duration::from_secs(30) {break;} },
            accepted=listener.accept()=> {
                let (s,_)=accepted.map_err(|_|"ipc_unavailable")?;
                if count.load(Ordering::SeqCst)>=8 {drop(s);continue;}
                count.fetch_add(1,Ordering::SeqCst); let count=count.clone();let rx=rx.clone();let retry=retry_tx.clone();let instance=instance.clone();
                tokio::spawn(async move {let _=client(s,rx,retry,instance).await;count.fetch_sub(1,Ordering::SeqCst);});
            }
        }
    }
    auth.abort();
    drop(listener);
    if standalone {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}
pub async fn bridge() -> Result<(), &'static str> {
    bridge_to(&socket_path()?, |line| protocol::request(line).map(|_| ())).await
}
/// Relays stdin requests to a daemon socket and its frames to stdout. Every
/// request is checked by `check` before it is forwarded.
pub(crate) async fn bridge_to(
    path: &std::path::Path,
    check: fn(&[u8]) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    socket_permissions(path)?;
    let s = timeout(IO_DEADLINE, UnixStream::connect(path))
        .await
        .map_err(|_| "daemon_unavailable")?
        .map_err(|_| "daemon_unavailable")?;
    peer_allowed(&s)?;
    let (incoming, mut outgoing) = s.into_split();
    let send = async {
        let mut input = BufReader::new(tokio::io::stdin());
        while let Some(line) = protocol::read_line(&mut input).await? {
            check(&line)?;
            timeout(IO_DEADLINE, outgoing.write_all(&line))
                .await
                .map_err(|_| "io_timeout")?
                .map_err(|_| "io_unavailable")?;
            timeout(IO_DEADLINE, outgoing.write_all(b"\n"))
                .await
                .map_err(|_| "io_timeout")?
                .map_err(|_| "io_unavailable")?;
        }
        Ok::<(), &'static str>(())
    };
    let receive = async {
        let mut input = BufReader::new(incoming);
        let mut output = tokio::io::stdout();
        while let Some(line) = protocol::read_response_line(&mut input).await? {
            let v: serde_json::Value =
                serde_json::from_slice(&line).map_err(|_| "invalid_response")?;
            write(&mut output, &v).await?;
            timeout(IO_DEADLINE, output.flush())
                .await
                .map_err(|_| "io_timeout")?
                .map_err(|_| "io_unavailable")?;
        }
        Ok::<(), &'static str>(())
    };
    tokio::select! {r=send=>r,r=receive=>r}
}

#[cfg(test)]
mod send_reply_tests {
    use super::*;
    #[tokio::test]
    async fn timeout_abandons_queued_reply_and_reports_unknown() {
        let (send, reply) = tokio::sync::oneshot::channel();
        assert_eq!(
            send_result(reply, Duration::from_millis(5), "delivery_unknown").await,
            Some("delivery_unknown")
        );
        assert!(send.is_closed());
    }
    #[tokio::test]
    async fn actor_loss_is_unknown_but_explicit_rejection_remains_exact() {
        let (send, reply) = tokio::sync::oneshot::channel();
        drop(send);
        assert_eq!(
            send_result(reply, Duration::from_secs(1), "delivery_unknown").await,
            Some("delivery_unknown")
        );
        let (send, reply) = tokio::sync::oneshot::channel();
        send.send(Some("send_request_reused")).unwrap();
        assert_eq!(
            send_result(reply, Duration::from_secs(1), "delivery_unknown").await,
            Some("send_request_reused")
        );
        let (_send, reply) = tokio::sync::oneshot::channel();
        assert_eq!(
            send_result(reply, Duration::from_millis(5), "dm_open_unknown").await,
            Some("dm_open_unknown")
        );
    }
}

#[cfg(test)]
mod setup_tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    type Lines = tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>;
    async fn next(lines: &mut Lines, seen: &mut String) -> serde_json::Value {
        let line = timeout(Duration::from_secs(30), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        seen.push_str(&line);
        serde_json::from_str(&line).unwrap()
    }
    async fn answer(lines: &mut Lines, seen: &mut String, id: &str) -> serde_json::Value {
        loop {
            let frame = next(lines, seen).await;
            if frame["id"] == id {
                return frame;
            }
        }
    }
    fn connect(
        rx: watch::Receiver<Status>,
        commands: mpsc::Sender<protocol::Command>,
    ) -> (Lines, tokio::net::unix::OwnedWriteHalf) {
        let (server, test) = UnixStream::pair().unwrap();
        tokio::spawn(client(server, rx, commands, "setup-fixture".into()));
        let (read, write) = test.into_split();
        (BufReader::new(read).lines(), write)
    }

    #[tokio::test]
    async fn setup_is_refused_while_authenticated_or_connecting() {
        let mut status = Status::new(&config::Config::default());
        status.connection = "authenticated".into();
        let (tx, rx) = watch::channel(status);
        let (commands, mut received) = mpsc::channel(1);
        let (mut lines, mut write) = connect(rx, commands);
        let mut seen = String::new();
        assert_eq!(next(&mut lines, &mut seen).await["type"], "hello");
        let relay = br#"{"version":1,"id":"ui-1","type":"set_relay","url":"wss://example.com"}"#;
        let create = br#"{"version":1,"id":"ui-2","type":"create_identity"}"#;
        for (connection, category) in [
            ("authenticated", "setup_not_allowed"),
            ("connecting", "setup_busy"),
        ] {
            tx.send_modify(|s| s.connection = connection.into());
            for (request, id) in [(&relay[..], "ui-1"), (&create[..], "ui-2")] {
                write.write_all(request).await.unwrap();
                write.write_all(b"\n").await.unwrap();
                let frame = answer(&mut lines, &mut seen, id).await;
                assert_eq!(
                    (frame["type"].as_str(), frame["category"].as_str()),
                    (Some("error"), Some(category)),
                    "{connection}"
                );
            }
        }
        tx.send_modify(|s| {
            s.connection = "unavailable".into();
            s.category = Some("identity_access_pending".into());
        });
        write.write_all(create).await.unwrap();
        write.write_all(b"\n").await.unwrap();
        assert_eq!(
            answer(&mut lines, &mut seen, "ui-2").await["category"],
            "setup_busy"
        );
        // A non-canonical relay is refused by category; the connection stays open.
        tx.send_modify(|s| {
            s.connection = "unconfigured".into();
            s.category = None;
        });
        for url in [
            "http://example.com",
            "ws://example.com",
            "wss://u@example.com",
        ] {
            let request = serde_json::json!({"version":1,"id":"ui-3","type":"set_relay","url":url});
            write
                .write_all(&serde_json::to_vec(&request).unwrap())
                .await
                .unwrap();
            write.write_all(b"\n").await.unwrap();
            assert_eq!(
                answer(&mut lines, &mut seen, "ui-3").await["category"],
                "setup_invalid_relay"
            );
        }
        write
            .write_all(b"{\"version\":1,\"id\":\"ui-4\",\"type\":\"get_snapshot\"}\n")
            .await
            .unwrap();
        assert_eq!(
            answer(&mut lines, &mut seen, "ui-4").await["type"],
            "status"
        );
        assert!(
            received.try_recv().is_err(),
            "a refused setup reached the actor"
        );
    }

    #[tokio::test]
    async fn created_identity_reaches_status_but_its_secret_never_does() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (setup, secrets, dir) = crate::setup::tests::fixture();
        let signer = nostr::Keys::generate().public_key();
        let (origin, server) = crate::setup::tests::info_server(
            200,
            serde_json::json!({"self": signer.to_hex()}).to_string(),
            1,
        )
        .await;
        let configured = config::Config {
            relay: Some(origin.clone()),
            identity: None,
        };
        config::save_to(&dir, &configured).unwrap();
        let (tx, rx) = watch::channel(Status::new(&configured));
        let (commands, mut received) = mpsc::channel(1);
        let actor_setup = setup.clone();
        let actor = tokio::spawn(async move {
            let mut retries = 0;
            while let Some(command) = received.recv().await {
                if crate::auth::offline_command(command, &tx, &actor_setup, None).await {
                    retries += 1;
                }
            }
            retries
        });
        let (mut lines, mut write) = connect(rx, commands);
        let mut seen = String::new();
        assert!(next(&mut lines, &mut seen).await["status"]["identity"].is_null());
        write
            .write_all(b"{\"version\":1,\"id\":\"ui-1\",\"type\":\"create_identity\"}\n")
            .await
            .unwrap();
        let frame = answer(&mut lines, &mut seen, "ui-1").await;
        server.await.unwrap();
        assert_eq!(frame["type"], "status", "{frame}");
        let identity = frame["status"]["identity"].as_str().unwrap().to_owned();
        assert_eq!(identity.len(), 64);
        assert_eq!(frame["status"]["relay"], origin.as_str());
        assert_eq!(frame["generation"], 2);
        // A second creation is refused; the relay can be changed, clearing it.
        write
            .write_all(b"{\"version\":1,\"id\":\"ui-2\",\"type\":\"create_identity\"}\n")
            .await
            .unwrap();
        assert_eq!(
            answer(&mut lines, &mut seen, "ui-2").await["category"],
            "identity_exists"
        );
        write
            .write_all(b"{\"version\":1,\"id\":\"ui-3\",\"type\":\"set_relay\",\"url\":\"wss://Other.example\"}\n")
            .await
            .unwrap();
        let changed = answer(&mut lines, &mut seen, "ui-3").await;
        assert_eq!(changed["status"]["relay"], "wss://other.example/");
        assert!(changed["status"]["identity"].is_null());
        let items = secrets.items.lock().unwrap();
        let secret = items.get(&format!("{origin}|{identity}")).unwrap();
        assert!(
            !seen.contains(secret.as_str()),
            "secret crossed the IPC boundary"
        );
        let key = nostr::Keys::parse(secret.as_str()).unwrap();
        let bech32 = nostr::ToBech32::to_bech32(key.secret_key()).unwrap();
        assert!(!seen.contains(&bech32));
        drop(items);
        drop(write);
        drop(lines);
        assert_eq!(actor.await.unwrap(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn mints_need_an_authenticated_idle_session() {
        let mut status = Status::new(&config::Config::default());
        status.connection = "unconfigured".into();
        let (tx, rx) = watch::channel(status);
        let (commands, mut received) = mpsc::channel(1);
        let (mut lines, mut write) = connect(rx, commands);
        let mut seen = String::new();
        assert_eq!(next(&mut lines, &mut seen).await["type"], "hello");
        let id = "22222222-2222-4222-8222-222222222222";
        let mint = serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":5,"expiresInHours":168}).to_string();
        for (connection, state, category) in [
            ("unconfigured", "idle", "relay_unavailable"),
            ("disconnected", "idle", "relay_unavailable"),
            ("connecting", "idle", "relay_unavailable"),
            ("authenticated", "minting", "setup_busy"),
        ] {
            tx.send_modify(|s| {
                s.connection = connection.into();
                s.invites.state = state.into();
            });
            write.write_all(mint.as_bytes()).await.unwrap();
            write.write_all(b"\n").await.unwrap();
            let frame = answer(&mut lines, &mut seen, id).await;
            assert_eq!(
                (frame["type"].as_str(), frame["category"].as_str()),
                (Some("error"), Some(category)),
                "{connection} {state}"
            );
        }
        assert!(
            received.try_recv().is_err(),
            "a refused mint reached the actor"
        );
        tx.send_modify(|s| {
            s.connection = "authenticated".into();
            s.invites.state = "failed".into();
        });
        write.write_all(mint.as_bytes()).await.unwrap();
        write.write_all(b"\n").await.unwrap();
        let command = timeout(Duration::from_secs(5), received.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(command, protocol::Command::MintInvite(5, 168, _)));
    }

    #[tokio::test]
    async fn transfers_need_an_authenticated_session_and_one_at_a_time() {
        let mut status = Status::new(&config::Config::default());
        status.connection = "connecting".into();
        let (tx, rx) = watch::channel(status);
        let (commands, mut received) = mpsc::channel(1);
        let (mut lines, mut write) = connect(rx, commands);
        let mut seen = String::new();
        assert_eq!(next(&mut lines, &mut seen).await["type"], "hello");
        let (event, hash) = ("a".repeat(64), "b".repeat(64));
        let room = "00000000-0000-4000-8000-000000000001";
        let download = serde_json::json!({"version":1,"id":"ui-1","type":"download_attachment","eventId":event,"hash":hash}).to_string();
        let upload = serde_json::json!({"version":1,"id":"ui-2","type":"upload_attachment","roomId":room,"path":"/home/u/a.png"}).to_string();
        let thumb = serde_json::json!({"version":1,"id":"ui-3","type":"thumbnail_attachment","eventId":event,"hash":hash}).to_string();
        for (request, id, download_state, upload_state, category) in [
            (&download, "ui-1", "idle", "idle", "relay_unavailable"),
            (&upload, "ui-2", "idle", "idle", "relay_unavailable"),
            (&thumb, "ui-3", "idle", "idle", "relay_unavailable"),
            (&download, "ui-1", "downloading", "idle", "setup_busy"),
            (&upload, "ui-2", "idle", "uploading", "setup_busy"),
        ] {
            tx.send_modify(|s| {
                s.connection = if category == "setup_busy" {
                    "authenticated"
                } else {
                    "connecting"
                }
                .into();
                s.download.state = download_state.into();
                s.upload.state = upload_state.into();
            });
            write.write_all(request.as_bytes()).await.unwrap();
            write.write_all(b"\n").await.unwrap();
            let frame = answer(&mut lines, &mut seen, id).await;
            assert_eq!(
                (frame["type"].as_str(), frame["category"].as_str()),
                (Some("error"), Some(category)),
                "{request}"
            );
        }
        assert!(
            received.try_recv().is_err(),
            "a refused transfer reached the actor"
        );
        tx.send_modify(|s| {
            s.connection = "authenticated".into();
            s.download.state = "done".into();
        });
        write.write_all(download.as_bytes()).await.unwrap();
        write.write_all(b"\n").await.unwrap();
        let command = timeout(Duration::from_secs(5), received.recv())
            .await
            .unwrap()
            .unwrap();
        let protocol::Command::DownloadAttachment(e, h, reply) = command else {
            panic!("not a download");
        };
        assert_eq!((e.as_str(), h.as_str()), (event.as_str(), hash.as_str()));
        reply.send(Some("attachment_unknown")).unwrap();
        let frame = answer(&mut lines, &mut seen, "ui-1").await;
        assert_eq!(frame["category"], "attachment_unknown");
        // Opening and removing work without the relay; the actor decides.
        let open = serde_json::json!({"version":1,"id":"ui-4","type":"open_download","path":"/home/u/Downloads/a.pdf"}).to_string();
        tx.send_modify(|s| s.connection = "disconnected".into());
        write.write_all(open.as_bytes()).await.unwrap();
        write.write_all(b"\n").await.unwrap();
        let command = timeout(Duration::from_secs(5), received.recv())
            .await
            .unwrap()
            .unwrap();
        let protocol::Command::OpenDownload(path, reply) = command else {
            panic!("not an open");
        };
        assert_eq!(path, "/home/u/Downloads/a.pdf");
        reply.send(None).unwrap();
        assert_eq!(
            answer(&mut lines, &mut seen, "ui-4").await["type"],
            "status"
        );
    }

    #[tokio::test]
    async fn invites_need_a_relay_identity_and_a_settled_connection() {
        let identity = nostr::Keys::generate().public_key().to_hex();
        let mut status = Status::new(&config::Config::default());
        status.connection = "unconfigured".into();
        let (tx, rx) = watch::channel(status);
        let (commands, mut received) = mpsc::channel(1);
        let (mut lines, mut write) = connect(rx, commands);
        let mut seen = String::new();
        assert_eq!(next(&mut lines, &mut seen).await["type"], "hello");
        let claim = br#"{"version":1,"id":"ui-1","type":"claim_invite","input":"code"}"#;
        let accept = br#"{"version":1,"id":"ui-2","type":"accept_invite","code":"code","policyVersion":null}"#;
        let mut refusals = vec![("unconfigured", false, "idle", "setup_not_allowed")];
        for connection in ["connecting"] {
            refusals.push((connection, true, "idle", "setup_busy"));
        }
        refusals.push(("unavailable", true, "idle", "setup_not_allowed"));
        refusals.push(("authenticated", true, "checking", "setup_busy"));
        refusals.push(("disconnected", true, "claiming", "setup_busy"));
        for (connection, configured, state, category) in refusals {
            tx.send_modify(|s| {
                s.connection = connection.into();
                s.relay = configured.then(|| "wss://relay.example/".to_string());
                s.identity = configured.then(|| identity.clone());
                s.setup.state = state.into();
            });
            for (request, id) in [(&claim[..], "ui-1"), (&accept[..], "ui-2")] {
                write.write_all(request).await.unwrap();
                write.write_all(b"\n").await.unwrap();
                let frame = answer(&mut lines, &mut seen, id).await;
                assert_eq!(
                    (frame["type"].as_str(), frame["category"].as_str()),
                    (Some("error"), Some(category)),
                    "{connection} {state}"
                );
            }
        }
        assert!(
            received.try_recv().is_err(),
            "a refused invite reached the actor"
        );
        // Disconnected (possibly refused as a non-member) with an identity: allowed.
        tx.send_modify(|s| {
            s.connection = "disconnected".into();
            s.category = Some("auth_rejected".into());
            s.setup.state = "failed".into();
        });
        write.write_all(claim).await.unwrap();
        write.write_all(b"\n").await.unwrap();
        let command = timeout(Duration::from_secs(5), received.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(command, protocol::Command::ClaimInvite(ref input, _) if input == "code"));
    }

    #[tokio::test]
    async fn an_invite_is_redeemed_while_disconnected_and_reconnects() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let keys = nostr::Keys::generate();
        let (relay, server) = crate::join::tests::http_fixture(vec![
            (404, String::new()),
            (200, crate::join::tests::CLAIMED.into()),
        ])
        .await;
        let configured = config::Config {
            relay: Some(relay.clone()),
            identity: Some(keys.public_key().to_hex()),
        };
        let mut status = Status::new(&configured);
        status.connection = "disconnected".into();
        status.category = Some("auth_rejected".into());
        let (tx, rx) = watch::channel(status);
        let (commands, mut received) = mpsc::channel(1);
        let (setup, _, dir) = crate::setup::tests::fixture();
        let actor_relay = relay.clone();
        let actor = tokio::spawn(async move {
            let mut retries = 0;
            while let Some(command) = received.recv().await {
                let session = Some((actor_relay.as_str(), &keys));
                if crate::auth::offline_command(command, &tx, &setup, session).await {
                    retries += 1;
                }
            }
            retries
        });
        let (mut lines, mut write) = connect(rx, commands);
        let mut seen = String::new();
        assert_eq!(next(&mut lines, &mut seen).await["type"], "hello");
        let code = crate::join::tests::CODE;
        let claim = serde_json::json!({"version":1,"id":"ui-1","type":"claim_invite","input":format!(" {code} ")});
        write
            .write_all(format!("{claim}\n").as_bytes())
            .await
            .unwrap();
        let frame = answer(&mut lines, &mut seen, "ui-1").await;
        assert_eq!(frame["type"], "status", "{frame}");
        assert_eq!(
            frame["status"]["setup"],
            serde_json::json!({"state":"policy","inviteCode":code,"joinPolicy":null,"claim":null,"category":null})
        );
        // Only the published code is accepted.
        let other = serde_json::json!({"version":1,"id":"ui-2","type":"accept_invite","code":"other","policyVersion":null});
        write
            .write_all(format!("{other}\n").as_bytes())
            .await
            .unwrap();
        assert_eq!(
            answer(&mut lines, &mut seen, "ui-2").await["category"],
            "invite_invalid"
        );
        let accept = serde_json::json!({"version":1,"id":"ui-3","type":"accept_invite","code":code,"policyVersion":null});
        write
            .write_all(format!("{accept}\n").as_bytes())
            .await
            .unwrap();
        let frame = answer(&mut lines, &mut seen, "ui-3").await;
        assert_eq!(frame["type"], "status", "{frame}");
        assert_eq!(
            frame["status"]["setup"],
            serde_json::json!({"state":"joined","inviteCode":null,"joinPolicy":null,"category":null,
                "claim":{"status":"joined","communityId":"11111111-1111-4111-8111-111111111111","host":"127.0.0.1","role":"member"}})
        );
        assert_eq!(server.await.unwrap().len(), 2);
        drop(write);
        drop(lines);
        // The successful claim asked the connection to be retried.
        assert_eq!(actor.await.unwrap(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
