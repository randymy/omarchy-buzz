//! Desktop notifications (`notify`): the panel hands the helper a notification
//! over the IPC pipe and the helper calls `org.freedesktop.Notifications.Notify`
//! on the session bus itself. Message previews, sender and room names never
//! reach a process argument (`omarchy notification send` and `busctl` put them
//! in argv, readable by other local users without `/proc` hidepid).
//!
//! The call mirrors `omarchy-notification-send` exactly: app name `Buzz`, no
//! replaced id, no icon, no actions, `urgency` (byte 1) and `omarchy-exec-argv`
//! (the click command as a JSON argv built only from validated ids) as hints,
//! and an 8 second expiry.
use dbus::{arg::RefArg, arg::Variant, blocking::Connection};
use std::{collections::HashMap, time::Duration};

/// Title and body caps in bytes, applied after sanitizing (before escaping).
pub const TITLE_BYTES: usize = 200;
pub const BODY_BYTES: usize = 300;
/// What the request itself may carry; the helper then truncates to the caps.
pub const RAW_TITLE_BYTES: usize = 800;
pub const RAW_BODY_BYTES: usize = 1200;
pub const EXPIRE_MS: i32 = 8000;
/// One `Notify` call waits this long for the notification server.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(3);
/// The whole attempt (connect included) is abandoned after this.
pub const DEADLINE: Duration = Duration::from_secs(5);
/// Notifications waiting for the bus (one runs at a time); more are refused.
pub const QUEUE: usize = 5;

/// A checked notification: sanitized text and validated click-target ids.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub room: String,
    pub thread: Option<String>,
}

/// One `Notify` hint value (`a{sv}`).
#[derive(Clone, Debug, PartialEq)]
pub enum Hint {
    Byte(u8),
    Text(String),
}

/// Every argument of `Notify` (`susssasa{sv}i`), in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub app_name: &'static str,
    pub replaces_id: u32,
    pub app_icon: &'static str,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub hints: Vec<(&'static str, Hint)>,
    pub expire_timeout: i32,
}

/// Why a request is refused before anything is shown. `None`: accepted.
/// `request` is the (instance, generation) the panel sent, `current` the
/// helper's own; `busy` is true when too many notifications are waiting.
pub fn refusal(
    request: (Option<&str>, Option<u64>),
    current: (&str, u64),
    busy: bool,
) -> Option<&'static str> {
    if request.0 != Some(current.0) || request.1 != Some(current.1) {
        Some("notify_scope_changed")
    } else if busy {
        Some("request_busy")
    } else {
        None
    }
}

/// The text of a notification as the panel sent it: controls and bidi
/// formatting become spaces (like every other relayed label) and the result is
/// cut at a character boundary. `None` for an empty title.
pub fn notice(title: &str, body: &str, room: &str, thread: Option<&str>) -> Option<Notice> {
    let title = crate::recipients::sanitize(title, TITLE_BYTES);
    if title.is_empty() {
        return None;
    }
    Some(Notice {
        title,
        body: crate::recipients::sanitize(body, BODY_BYTES),
        room: room.to_owned(),
        thread: thread.map(str::to_owned),
    })
}

/// The notification server renders the body as styled text (tags, and images it
/// would fetch), so message text is escaped to stay literal. This is the only
/// place that does it; the panel sends the plain text.
pub fn escape_body(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The click command: `omarchy-shell` summons the panel with the target, which
/// carries only the room id and, for a thread reply, the thread root id.
pub fn click_argv(notice: &Notice) -> String {
    let mut target = serde_json::Map::new();
    target.insert("room".into(), notice.room.clone().into());
    if let Some(thread) = &notice.thread {
        target.insert("thread".into(), thread.clone().into());
    }
    serde_json::json!([
        "omarchy-shell",
        "-q",
        "shell",
        "summon",
        "community.buzz",
        serde_json::Value::Object(target).to_string()
    ])
    .to_string()
}

/// The exact `Notify` arguments for a notice (pure, so it is unit-tested).
pub fn call(notice: &Notice) -> Call {
    Call {
        app_name: "Buzz",
        replaces_id: 0,
        app_icon: "",
        summary: notice.title.clone(),
        body: escape_body(&notice.body),
        actions: Vec::new(),
        hints: vec![
            ("urgency", Hint::Byte(1)),
            ("omarchy-exec-argv", Hint::Text(click_argv(notice))),
        ],
        expire_timeout: EXPIRE_MS,
    }
}

/// Calls `Notify` on `connection` and returns the server's notification id.
/// Failures are bounded categories, never the text or the bus error message.
pub fn deliver(connection: &Connection, call: &Call) -> Result<u32, &'static str> {
    let hints: HashMap<String, Variant<Box<dyn RefArg>>> = call
        .hints
        .iter()
        .map(|(key, hint)| {
            let value: Box<dyn RefArg> = match hint {
                Hint::Byte(byte) => Box::new(*byte),
                Hint::Text(text) => Box::new(text.clone()),
            };
            ((*key).to_owned(), Variant(value))
        })
        .collect();
    let proxy = connection.with_proxy(
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        CALL_TIMEOUT,
    );
    let (id,): (u32,) = proxy
        .method_call(
            "org.freedesktop.Notifications",
            "Notify",
            (
                call.app_name,
                call.replaces_id,
                call.app_icon,
                call.summary.as_str(),
                call.body.as_str(),
                call.actions.clone(),
                hints,
                call.expire_timeout,
            ),
        )
        .map_err(|error| {
            if error.name() == Some("org.freedesktop.DBus.Error.NoReply") {
                "notify_timeout"
            } else {
                "notify_failed"
            }
        })?;
    Ok(id)
}

/// Opens the session bus, asks `current` (the scope recheck) once connected and
/// only then delivers, so a slow connect never shows a stale notice. Blocking:
/// run it off the async loop. The `dbus` crate has no connect or authentication
/// timeout (only the method call has one, [`CALL_TIMEOUT`]); the caller instead
/// keeps its queue slot until this returns, which bounds a stall to one thread.
pub fn send(call: &Call, current: &dyn Fn() -> Option<&'static str>) -> Result<u32, &'static str> {
    let connection = Connection::new_session().map_err(|_| "notify_unavailable")?;
    if let Some(category) = current() {
        return Err(category);
    }
    deliver(&connection, call)
}

/// Notifications run one at a time, in order, and never block the async loop.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static WAITING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// True when [`QUEUE`] notifications are already waiting for the bus.
pub fn busy() -> bool {
    WAITING.load(std::sync::atomic::Ordering::SeqCst) >= QUEUE
}

/// Whether a queued notification may still be shown: the session is the one it
/// was made in (generation), is authenticated, and still lists the room. Checked
/// again right before delivery, because a notification can wait behind a slow
/// notification server while the community, connection or room access changes.
pub fn still_current(
    status: &crate::protocol::Status,
    generation: u64,
    room: &str,
) -> Option<&'static str> {
    if status.generation != generation
        || status.connection != "authenticated"
        || !status.catalog.rooms.iter().any(|r| r.id == room)
    {
        Some("notify_scope_changed")
    } else {
        None
    }
}

type Sender = fn(&Call, &dyn Fn() -> Option<&'static str>) -> Result<u32, &'static str>;

/// One counted place in the queue; released when dropped.
struct Slot;
impl Drop for Slot {
    fn drop(&mut self) {
        WAITING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Delivers in the background. The panel is answered before the bus is asked,
/// so a stuck notification server never delays other requests; a failure is
/// reported on stderr as its category only (never the text). The scope is
/// rechecked inside the blocking work, after the bus is connected and right
/// before `Notify`, and a notice that is no longer current is dropped. The turn
/// and the queue slot move into that blocking work: they are released only when
/// it really returns, so at most one blocking D-Bus operation exists and the
/// queue limit counts it. The async side stops waiting after [`DEADLINE`] (the
/// IPC loop is never blocked) but cannot release them early.
pub fn spawn(
    notice: Notice,
    generation: u64,
    status: tokio::sync::watch::Receiver<crate::protocol::Status>,
) {
    spawn_with(notice, generation, status, send, DEADLINE);
}

fn spawn_with(
    notice: Notice,
    generation: u64,
    status: tokio::sync::watch::Receiver<crate::protocol::Status>,
    sender: Sender,
    deadline: Duration,
) {
    WAITING.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let slot = Slot;
    tokio::spawn(async move {
        let turn = TURN.lock().await;
        let call = call(&notice);
        let room = notice.room;
        let work = tokio::task::spawn_blocking(move || {
            let _held = (turn, slot);
            sender(&call, &|| {
                still_current(&status.borrow(), generation, &room)
            })
        });
        let result = match tokio::time::timeout(deadline, work).await {
            Ok(Ok(result)) => result.map(|_| ()),
            Ok(Err(_)) => Err("notify_failed"),
            Err(_) => Err("notify_timeout"),
        };
        if let Err(category) = result {
            eprintln!("{category}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbus::{
        channel::{Channel, MatchingReceiver},
        message::MatchRule,
    };
    use std::{
        io::{BufRead, BufReader},
        sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex},
    };
    const ROOM: &str = "11111111-1111-4111-8111-111111111111";
    const THREAD: &str = "c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0";

    fn plain() -> Notice {
        notice("Alex mentioned you in #general", "hi", ROOM, None).unwrap()
    }

    #[test]
    fn sanitizes_and_bounds_text() {
        let n = notice("a\u{0}b\nc\u{202e}d", "x\ty\u{2066}z", ROOM, None).unwrap();
        assert_eq!((n.title.as_str(), n.body.as_str()), ("a b c d", "x y z"));
        let long = "é".repeat(400);
        let n = notice(&long, &long, ROOM, None).unwrap();
        assert!((TITLE_BYTES - 1..=TITLE_BYTES).contains(&n.title.len()));
        assert!((BODY_BYTES - 1..=BODY_BYTES).contains(&n.body.len()));
        assert!(notice(" \n\u{200e} ", "body", ROOM, None).is_none());
        assert_eq!(notice("t", "", ROOM, None).unwrap().body, "");
    }

    #[test]
    fn body_is_escaped_once_and_title_is_not() {
        let n = notice("<b>t</b>", "<img src=x> & you", ROOM, None).unwrap();
        let c = call(&n);
        assert_eq!(c.body, "&lt;img src=x&gt; &amp; you");
        assert_eq!(c.summary, "<b>t</b>");
        // The stored notice stays plain: escaping again would double it.
        assert_eq!(n.body, "<img src=x> & you");
    }

    #[test]
    fn call_mirrors_the_omarchy_sender() {
        let c = call(&plain());
        assert_eq!(
            (c.app_name, c.replaces_id, c.app_icon, c.expire_timeout),
            ("Buzz", 0, "", 8000)
        );
        assert!(c.actions.is_empty());
        assert_eq!(c.hints.len(), 2);
        assert_eq!(c.hints[0], ("urgency", Hint::Byte(1)));
        assert_eq!(c.hints[1].0, "omarchy-exec-argv");
        let Hint::Text(argv) = &c.hints[1].1 else {
            panic!("click hint is text")
        };
        let parsed: Vec<String> = serde_json::from_str(argv).unwrap();
        assert_eq!(
            parsed,
            vec![
                "omarchy-shell",
                "-q",
                "shell",
                "summon",
                "community.buzz",
                &format!("{{\"room\":\"{ROOM}\"}}")
            ]
        );
    }

    #[test]
    fn thread_target_carries_the_root_and_no_text() {
        let n = notice("Alex replied", "secret", ROOM, Some(THREAD)).unwrap();
        let argv: Vec<String> = serde_json::from_str(&click_argv(&n)).unwrap();
        assert_eq!(
            argv[5],
            format!("{{\"room\":\"{ROOM}\",\"thread\":\"{THREAD}\"}}")
        );
        assert!(!click_argv(&n).contains("secret") && !click_argv(&n).contains("Alex"));
    }

    #[test]
    fn stale_scope_and_backlog_are_refused() {
        assert_eq!(refusal((Some("i"), Some(2)), ("i", 2), false), None);
        for stale in [
            (Some("other"), Some(2)),
            (Some("i"), Some(3)),
            (None, Some(2)),
            (Some("i"), None),
        ] {
            assert_eq!(
                refusal(stale, ("i", 2), false),
                Some("notify_scope_changed")
            );
            assert_eq!(refusal(stale, ("i", 2), true), Some("notify_scope_changed"));
        }
        assert_eq!(
            refusal((Some("i"), Some(2)), ("i", 2), true),
            Some("request_busy")
        );
    }

    /// A private bus, or None (loudly) when `dbus-daemon` is not installed.
    fn private_bus() -> Option<(std::process::Child, String)> {
        let spawned = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        let Ok(mut daemon) = spawned else {
            eprintln!("SKIPPED: dbus-daemon is not installed");
            return None;
        };
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Some((daemon, address.trim().to_owned()))
    }
    fn connect(address: &str) -> Connection {
        let mut channel = Channel::open_private(address).unwrap();
        channel.register().unwrap();
        Connection::from(channel)
    }

    /// Runs `body` against a private bus whose fake notification server records
    /// the arguments it receives and answers `reply` (an id, or an error).
    /// Milliseconds the fake server waits before answering (a stalled server).
    pub(super) static STALL_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub(super) fn with_fake_server(
        reply: Option<u32>,
        body: impl FnOnce(&Connection, &Mutex<Vec<String>>, &str),
    ) -> bool {
        let Some((mut daemon, address)) = private_bus() else {
            return false;
        };
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let server = connect(&address);
        server
            .request_name("org.freedesktop.Notifications", false, true, false)
            .unwrap();
        let record = seen.clone();
        server.start_receive(
            MatchRule::new_method_call(),
            Box::new(move |message, connection| {
                let mut items = message.iter_init();
                let mut fields = Vec::new();
                while let Some(arg) = items.get_refarg() {
                    fields.push(format!("{arg:?}"));
                    items.next();
                }
                record.lock().unwrap().push(fields.join("|"));
                std::thread::sleep(Duration::from_millis(STALL_MS.load(Ordering::SeqCst)));
                let _ = match reply {
                    Some(id) => connection
                        .channel()
                        .send(message.method_return().append1(id)),
                    None => connection.channel().send(message.error(
                        &"org.freedesktop.DBus.Error.Failed".into(),
                        &std::ffi::CString::new("refused").unwrap(),
                    )),
                };
                true
            }),
        );
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                let _ = server.process(Duration::from_millis(20));
            }
        });
        body(&connect(&address), &seen, &address);
        stop.store(true, Ordering::SeqCst);
        thread.join().unwrap();
        let _ = daemon.kill();
        let _ = daemon.wait();
        true
    }

    #[test]
    fn notify_reaches_a_fake_server_with_every_argument() {
        let n = notice("Alex in #general", "a <b> & c", ROOM, Some(THREAD)).unwrap();
        with_fake_server(Some(42), |client, seen, _| {
            assert_eq!(deliver(client, &call(&n)), Ok(42));
            let seen = seen.lock().unwrap();
            assert_eq!(seen.len(), 1);
            let wire = &seen[0];
            for part in [
                "Buzz",
                "Alex in #general",
                "a &lt;b&gt; &amp; c",
                "omarchy-exec-argv",
                "urgency",
                "community.buzz",
                THREAD,
                "8000",
            ] {
                assert!(wire.contains(part), "{part} missing from {wire}");
            }
            assert!(!wire.contains("a <b>"), "body was not escaped: {wire}");
        });
    }

    #[test]
    fn server_error_and_missing_server_are_bounded_categories() {
        with_fake_server(None, |client, _, _| {
            assert_eq!(deliver(client, &call(&plain())), Err("notify_failed"));
        });
        // A bus where nobody owns the notification name.
        if let Some((mut daemon, address)) = private_bus() {
            assert_eq!(
                deliver(&connect(&address), &call(&plain())),
                Err("notify_failed")
            );
            let _ = daemon.kill();
            let _ = daemon.wait();
        }
        // No bus at all: the session address points nowhere.
        assert!(Channel::open_private("unix:path=/nonexistent/omarchy-buzz-bus").is_err());
    }
}

#[cfg(test)]
mod unavailable_tests {
    use super::*;
    #[test]
    fn no_session_bus_is_notify_unavailable() {
        // Tests run one at a time (RUST_TEST_THREADS=1); the variable is restored.
        let key = "DBUS_SESSION_BUS_ADDRESS";
        let saved = std::env::var_os(key);
        std::env::set_var(key, "unix:path=/nonexistent/omarchy-buzz-bus");
        let notice = notice(
            "Alex",
            "private",
            "11111111-1111-4111-8111-111111111111",
            None,
        );
        let result = send(&call(&notice.unwrap()), &|| None);
        match saved {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        assert_eq!(result, Err("notify_unavailable"));
    }
}

#[cfg(test)]
mod queued_tests {
    use super::*;
    use crate::protocol::{Room, Status};
    const ROOM: &str = "11111111-1111-4111-8111-111111111111";
    pub(super) static ADDRESS: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

    fn status() -> Status {
        let mut s = Status::new(&crate::config::Config::default());
        s.connection = "authenticated".into();
        s.generation = 4;
        s.catalog.rooms.push(Room {
            id: ROOM.into(),
            name: "general".into(),
            description: String::new(),
            kind: "stream".into(),
            participants: Vec::new(),
            hidden: false,
        });
        s
    }
    pub(super) static IN_FLIGHT: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);
    pub(super) static MOST: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    pub(super) static CONNECT_MS: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);
    pub(super) fn to_private_bus(
        call: &Call,
        current: &dyn Fn() -> Option<&'static str>,
    ) -> Result<u32, &'static str> {
        use std::sync::atomic::Ordering::SeqCst;
        let now = IN_FLIGHT.fetch_add(1, SeqCst) + 1;
        MOST.fetch_max(now, SeqCst);
        // A slow connect, then the scope recheck, then the call (as `send` does).
        std::thread::sleep(Duration::from_millis(CONNECT_MS.load(SeqCst)));
        let address = ADDRESS.lock().unwrap().clone();
        let mut channel = dbus::channel::Channel::open_private(&address).unwrap();
        channel.register().unwrap();
        let connection = Connection::from(channel);
        let result = match current() {
            Some(category) => Err(category),
            None => deliver(&connection, call),
        };
        IN_FLIGHT.fetch_sub(1, SeqCst);
        result
    }

    #[test]
    fn current_needs_generation_authentication_and_room() {
        let s = status();
        assert_eq!(still_current(&s, 4, ROOM), None);
        assert!(still_current(&s, 3, ROOM).is_some());
        assert!(still_current(&s, 4, "22222222-2222-4222-8222-222222222222").is_some());
        let mut out = s.clone();
        out.connection = "disconnected".into();
        assert!(still_current(&out, 4, ROOM).is_some());
        let mut gone = s;
        gone.catalog.rooms.clear();
        assert!(still_current(&gone, 4, ROOM).is_some());
    }

    /// Queues one notification behind a held turn (a slow notification server),
    /// lets `change` alter the status meanwhile, and counts what the server got.
    fn delivered_after(change: impl FnOnce(&mut Status)) -> usize {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut total = 0;
        super::tests::with_fake_server(Some(7), |_, seen, address| {
            *ADDRESS.lock().unwrap() = address.to_owned();
            runtime.block_on(async {
                let (tx, rx) = tokio::sync::watch::channel(status());
                let turn = TURN.lock().await;
                let n = notice("Alex", "private preview", ROOM, None).unwrap();
                spawn_with(n, 4, rx, to_private_bus, DEADLINE);
                tokio::task::yield_now().await;
                tx.send_modify(change);
                drop(turn);
                for _ in 0..50 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    if WAITING.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                        break;
                    }
                }
            });
            total = seen.lock().unwrap().len();
        });
        total
    }

    #[test]
    fn a_queued_notification_is_dropped_when_the_scope_changes() {
        assert_eq!(delivered_after(|s| s.generation = 5), 0);
        assert_eq!(delivered_after(|s| s.catalog.rooms.clear()), 0);
        assert_eq!(delivered_after(|s| s.connection = "disconnected".into()), 0);
    }

    #[test]
    fn a_queued_notification_in_the_same_scope_is_delivered() {
        assert_eq!(delivered_after(|_| {}), 1);
    }
}

#[cfg(test)]
mod stalled_tests {
    use super::*;
    use crate::protocol::{Room, Status};
    use std::sync::atomic::Ordering::SeqCst;
    const ROOM: &str = "11111111-1111-4111-8111-111111111111";

    fn status() -> Status {
        let mut s = Status::new(&crate::config::Config::default());
        s.connection = "authenticated".into();
        s.generation = 4;
        s.catalog.rooms.push(Room {
            id: ROOM.into(),
            name: "general".into(),
            description: String::new(),
            kind: "stream".into(),
            participants: Vec::new(),
            hidden: false,
        });
        s
    }
    fn reset() {
        super::queued_tests::IN_FLIGHT.store(0, SeqCst);
        super::queued_tests::MOST.store(0, SeqCst);
        super::queued_tests::CONNECT_MS.store(0, SeqCst);
        super::tests::STALL_MS.store(0, SeqCst);
    }
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }
    fn queue(rx: &tokio::sync::watch::Receiver<Status>, count: usize, deadline: Duration) {
        for _ in 0..count {
            let n = notice("Alex", "private", ROOM, None).unwrap();
            spawn_with(
                n,
                4,
                rx.clone(),
                super::queued_tests::to_private_bus,
                deadline,
            );
        }
    }
    async fn idle() {
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if WAITING.load(SeqCst) == 0 {
                return;
            }
        }
        panic!("notifications never finished");
    }

    #[test]
    fn a_stalled_server_never_has_two_blocking_calls_or_frees_its_slot() {
        reset();
        super::tests::STALL_MS.store(700, SeqCst);
        let mut delivered = 0;
        super::tests::with_fake_server(Some(1), |_, seen, address| {
            *super::queued_tests::ADDRESS.lock().unwrap() = address.to_owned();
            runtime().block_on(async {
                let (_tx, rx) = tokio::sync::watch::channel(status());
                // The async side gives up after 100 ms; the first call stalls 700 ms.
                queue(&rx, 3, Duration::from_millis(100));
                tokio::time::sleep(Duration::from_millis(300)).await;
                assert_eq!(WAITING.load(SeqCst), 3, "a timed-out call freed its slot");
                queue(&rx, 2, Duration::from_millis(100));
                assert!(busy(), "the queue limit does not count the stalled call");
                super::tests::STALL_MS.store(0, SeqCst);
                idle().await;
            });
            delivered = seen.lock().unwrap().len();
        });
        assert_eq!(
            super::queued_tests::MOST.load(SeqCst),
            1,
            "more than one blocking call at once"
        );
        assert_eq!(delivered, 5);
    }

    #[test]
    fn a_slow_connect_cannot_deliver_a_stale_notice() {
        reset();
        super::queued_tests::CONNECT_MS.store(400, SeqCst);
        let mut delivered = usize::MAX;
        super::tests::with_fake_server(Some(1), |_, seen, address| {
            *super::queued_tests::ADDRESS.lock().unwrap() = address.to_owned();
            runtime().block_on(async {
                let (tx, rx) = tokio::sync::watch::channel(status());
                // The caller stops waiting after 50 ms; the connect finishes later.
                queue(&rx, 1, Duration::from_millis(50));
                tokio::time::sleep(Duration::from_millis(150)).await;
                tx.send_modify(|s| s.generation = 5);
                idle().await;
            });
            delivered = seen.lock().unwrap().len();
        });
        assert_eq!(delivered, 0);
    }
}
