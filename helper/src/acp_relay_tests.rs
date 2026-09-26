//! Opt-in synthetic ACP protocol routing, never a provider/AI validation claim.
use buzz_sdk::{
    builders::{build_add_member, build_create_channel, build_message},
    nip_oa,
};
use buzz_ws_client::{NostrWsConnection, RelayMessage};
use nostr::{Event, Keys};
use serde_json::json;
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

struct Supervisor(Child);
impl Supervisor {
    fn alive(&mut self) {
        assert!(
            self.0.try_wait().unwrap().is_none(),
            "synthetic supervisor exited"
        );
    }
    async fn stop(&mut self) {
        self.0.stdin.as_mut().unwrap().write_all(b"stop\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success(), "supervisor cleanup failed");
                break;
            }
            assert!(Instant::now() < deadline, "supervisor stop deadline");
            sleep(Duration::from_millis(50)).await;
        }
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn input(name: &str) -> PathBuf {
    let path =
        PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("explicit {name} required")));
    assert!(
        path.is_absolute(),
        "fixture executable/source paths must be absolute"
    );
    path
}
async fn publish(conn: &mut NostrWsConnection, event: Event) {
    let id = event.id.to_hex();
    let ok = timeout(Duration::from_secs(10), conn.send_event(event))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ok.event_id, id);
    assert!(ok.accepted, "synthetic fixture event rejected");
}
fn token() -> String {
    format!("SYNTHETIC-{}", Uuid::new_v4())
}

#[tokio::test]
#[ignore = "requires passed disposable real-relay gate and built pinned synthetic ACP prerequisites"]
async fn acp_relay_synthetic_routing() {
    let _serial = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(120),async {
        let relay=std::env::var("OMARCHY_BUZZ_TEST_RELAY_URL").expect("explicit disposable fixture relay required");
        let url=url::Url::parse(&relay).unwrap();assert_eq!(url.scheme(),"ws");
        assert!(matches!(url.host_str(),Some("127.0.0.1"|"[::1]")) && url.port().is_some());
        assert!(url.username().is_empty()&&url.password().is_none()&&url.query().is_none()&&url.fragment().is_none()&&(url.path().is_empty()||url.path()=="/"));
        let relay=crate::config::canonical_relay(&relay).unwrap();
        let owner=Keys::parse("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        let agent=Keys::parse("0000000000000000000000000000000000000000000000000000000000000002").unwrap();
        let outsider=Keys::generate();let room=Uuid::new_v4();
        let mut admin=NostrWsConnection::connect_authenticated(&relay,&owner,None).await.unwrap();
        publish(&mut admin,build_create_channel(room,&format!("synthetic-acp-{room}"),Some(buzz_sdk::Visibility::Private),Some(buzz_sdk::ChannelKind::Stream),None,None).unwrap().sign_with_keys(&owner).unwrap()).await;
        for member in [agent.public_key(),outsider.public_key()] {
            publish(&mut admin,build_add_member(room,&member.to_hex(),None).unwrap().sign_with_keys(&owner).unwrap()).await;
        }
        let attestation=nip_oa::compute_auth_tag(&owner,&agent.public_key(),"kind=9").unwrap();
        let tag=nip_oa::parse_auth_tag(&attestation).unwrap();
        let registration=NostrWsConnection::connect_authenticated(&relay,&agent,Some(&tag)).await.expect("synthetic ownership NIP-OA auth");
        registration.disconnect().await.unwrap();
        let sid=Uuid::new_v4().to_string();
        admin.send_raw(&json!(["REQ",sid,{"kinds":[9],"#h":[room.to_string()],"authors":[agent.public_key().to_hex()]}])).await.unwrap();
        timeout(Duration::from_secs(5),async {loop {if let RelayMessage::Eose{subscription_id}=admin.next_event(Duration::from_secs(5)).await.unwrap(){if subscription_id==sid{break;}}}}).await.unwrap();
        let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let mut supervisor=Supervisor(Command::new("/usr/bin/python3").arg(root.join("scripts/acp-fixture"))
            .arg("--buzz-source").arg(input("OMARCHY_BUZZ_TEST_ACP_BUZZ_SOURCE"))
            .arg("--buzz-bin-dir").arg(input("OMARCHY_BUZZ_TEST_ACP_BIN_DIR"))
            .arg("--node").arg(input("OMARCHY_BUZZ_TEST_ACP_NODE"))
            .args(["--relay-url",&relay,"--room",&room.to_string(),"--fixture-auth-tag",&attestation,"--deadline","100"])
            .env_clear().env("PATH","/usr/bin:/bin").env("HOME","/nonexistent")
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("spawn isolated synthetic ACP supervisor"));
        let key=agent.public_key().to_hex();let mut positive=Vec::new();let ready_deadline=Instant::now()+Duration::from_secs(45);
        let mut next_send=Instant::now();
        // Process creation is not readiness. Only a verified same-room agent ACK
        // to an exact-key owner mention establishes the positive route.
        loop {
            supervisor.alive();assert!(Instant::now()<ready_deadline,"synthetic ACP signed ACK readiness deadline");
            if Instant::now()>=next_send {
                let id=token();let content=format!("AE-ID:{id}");positive.push(id);
                publish(&mut admin,build_message(room,&content,None,&[&key],false,&[],&[]).unwrap().sign_with_keys(&owner).unwrap()).await;
                next_send=Instant::now()+Duration::from_secs(3);
            }
            match admin.next_event(Duration::from_millis(200)).await {
                Ok(RelayMessage::Event{subscription_id,event}) if subscription_id==sid => {
                    event.verify().unwrap();assert_eq!(event.pubkey,agent.public_key());
                    assert!(event.tags.iter().any(|t|t.as_slice()==["h",room.to_string().as_str()]));
                    if event.content.contains("AE-ACK:")&&positive.iter().any(|t|event.content.contains(t)){break;}
                },Err(buzz_ws_client::WsClientError::Timeout)=>{},Err(_)=>panic!("synthetic observation transport failed"),_=>{}
            }
        }
        let unmentioned=token();let stranger=token();
        publish(&mut admin,build_message(room,&format!("AE-ID:{unmentioned}"),None,&[],false,&[],&[]).unwrap().sign_with_keys(&owner).unwrap()).await;
        let mut outsider_conn=NostrWsConnection::connect_authenticated(&relay,&outsider,None).await.unwrap();
        publish(&mut outsider_conn,build_message(room,&format!("AE-ID:{stranger}"),None,&[&key],false,&[],&[]).unwrap().sign_with_keys(&outsider).unwrap()).await;
        // A bounded absence observation, not a proof of permanent exclusion.
        // Do not append another positive prompt: its historical context could
        // legitimately contain negative AE-ID tokens copied by this fake peer.
        let quiet=Instant::now()+Duration::from_secs(4);
        while Instant::now()<quiet {
            supervisor.alive();
            match admin.next_event(Duration::from_millis(200)).await {
                Ok(RelayMessage::Event{subscription_id,event}) if subscription_id==sid=>{
                    event.verify().unwrap();assert_eq!(event.pubkey,agent.public_key());
                    assert!(!event.content.contains(&unmentioned)&&!event.content.contains(&stranger),"synthetic ACP routed excluded trigger");
                },Err(buzz_ws_client::WsClientError::Timeout)=>{},Err(_)=>panic!("negative observation transport failed"),_=>{}
            }
        }
        supervisor.stop().await;outsider_conn.disconnect().await.unwrap();admin.disconnect().await.unwrap();
    }).await.expect("bounded synthetic ACP routing fixture");
}
