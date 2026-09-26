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
    let d = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("runtime_unavailable")?);
    let m = std::fs::symlink_metadata(&d).map_err(|_| "runtime_unavailable")?;
    if !d.is_absolute()
        || !m.is_dir()
        || m.uid() != rustix::process::getuid().as_raw()
        || m.mode() & 0o077 != 0
    {
        return Err("runtime_insecure");
    }
    Ok(d.join("omarchy-buzz/control.sock"))
}
fn private_dir(path: &std::path::Path) -> Result<(), &'static str> {
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
fn socket_permissions(path: &std::path::Path) -> Result<(), &'static str> {
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
async fn write<W: tokio::io::AsyncWrite + Unpin>(
    w: &mut W,
    v: &serde_json::Value,
) -> Result<(), &'static str> {
    let mut b = serde_json::to_vec(v).map_err(|_| "invalid_response")?;
    if b.len() + 1 > protocol::LIMIT {
        return Err("oversized_response");
    }
    b.push(b'\n');
    timeout(IO_DEADLINE, w.write_all(&b))
        .await
        .map_err(|_| "io_timeout")?
        .map_err(|_| "io_unavailable")
}
async fn client(
    s: UnixStream,
    mut status: watch::Receiver<Status>,
    retry: mpsc::Sender<protocol::Command>,
    instance: String,
) -> Result<(), &'static str> {
    if s.peer_cred().map_err(|_| "peer_unavailable")?.uid() != rustix::process::getuid().as_raw() {
        return Err("peer_denied");
    }
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
                let command=match r.kind.as_str() {
                    "retry_connection"=>Some(protocol::Command::Retry),
                    "fetch_recent"=>Some(protocol::Command::FetchRecent(r.room_id.clone().unwrap())),
                    "fetch_recipients"=>Some(protocol::Command::FetchRecipients(r.room_id.clone().unwrap())),
                    "send_message"=>Some(protocol::Command::Send(protocol::SendIntent {
                        request_id:r.id.clone(),room:r.room_id.clone().unwrap(),text:r.text.clone().unwrap(),
                        mentions:r.mentions.clone().unwrap(),generation:r.generation.unwrap(),
                    })),
                    _=>None,
                };
                if let Some(command)=command {if retry.try_send(command).is_err() {write(&mut out,&serde_json::json!({"version":1,"type":"error","id":r.id,"category":"request_busy","instanceId":instance})).await?;continue;}}
                if r.kind=="subscribe" { subscribed=true; }
                let snapshot=status.borrow().clone();
                write(&mut out,&protocol::envelope("status",Some(&r.id),&instance,&snapshot)).await?;
            },
            changed=status.changed(), if subscribed=> { changed.map_err(|_|"daemon_unavailable")?; let snapshot=status.borrow_and_update().clone(); write(&mut out,&protocol::envelope("status",None,&instance,&snapshot)).await?; }
        }
    }
}
pub async fn daemon(keep: bool) -> Result<(), &'static str> {
    let c = config::load()?;
    let path = socket_path()?;
    private_dir(&path)?;
    let mut inherited = listenfd::ListenFd::from_env();
    let inherited_listener = inherited
        .take_unix_listener(0)
        .map_err(|_| "activation_invalid")?;
    let standalone = inherited_listener.is_none();
    let listener = if let Some(l) = inherited_listener {
        if l.local_addr()
            .map_err(|_| "activation_invalid")?
            .as_pathname()
            != Some(path.as_path())
        {
            return Err("activation_invalid");
        }
        socket_permissions(&path)?;
        l.set_nonblocking(true).map_err(|_| "activation_invalid")?;
        UnixListener::from_std(l).map_err(|_| "activation_invalid")?
    } else {
        // Never unlink an existing socket: its daemon may still be alive.
        let l = UnixListener::bind(&path).map_err(|_| "daemon_already_running")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "runtime_unavailable")?;
        l
    };
    let (tx, rx) = watch::channel(Status::new(&c));
    let (retry_tx, retry_rx) = mpsc::channel(1);
    let auth = tokio::spawn(crate::auth::run(c, tx, retry_rx));
    let count = Arc::new(AtomicUsize::new(0));
    let instance = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "clock_unavailable")?
            .as_nanos()
    );
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
    let path = socket_path()?;
    socket_permissions(&path)?;
    let s = timeout(IO_DEADLINE, UnixStream::connect(path))
        .await
        .map_err(|_| "daemon_unavailable")?
        .map_err(|_| "daemon_unavailable")?;
    if s.peer_cred().map_err(|_| "peer_unavailable")?.uid() != rustix::process::getuid().as_raw() {
        return Err("peer_denied");
    }
    let (incoming, mut outgoing) = s.into_split();
    let send = async {
        let mut input = BufReader::new(tokio::io::stdin());
        while let Some(line) = protocol::read_line(&mut input).await? {
            protocol::request(&line)?;
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
        while let Some(line) = protocol::read_line(&mut input).await? {
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
