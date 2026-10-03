#![forbid(unsafe_code)]
#[cfg(test)]
mod acp_relay_tests;
mod activity;
mod agents;
mod agents_service;
mod attachments;
mod auth;
mod catalog;
mod clock;
mod communities;
mod compatibility;
mod config;
mod dm_open;
mod enrollment;
mod history;
mod invites;
mod ipc;
mod join;
mod ledger;
mod live;
mod media;
mod presence;
mod protocol;
mod query;
#[cfg(test)]
mod real_relay_tests;
mod recipients;
mod rooms;
mod sending;
mod setup;
mod thread;
mod user_status;
// Network fixtures share the production concurrency budgets. Serialize fixtures,
// while individual tests still exercise multiple simultaneous requests explicitly.
#[cfg(test)]
static NETWORK_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
async fn run() -> Result<(), &'static str> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice(){
        ["daemon"]=>ipc::daemon(false).await,
        ["daemon","--keep-running"]=>ipc::daemon(true).await,
        ["ui-bridge"]=>ipc::bridge().await,
        ["agents-daemon"]=>agents_service::daemon(false).await,
        ["agents-daemon","--keep-running"]=>agents_service::daemon(true).await,
        ["agents-bridge"]=>agents_service::bridge().await,
        ["--version"]=>{println!("{}",serde_json::json!({"helperVersion":env!("CARGO_PKG_VERSION"),"protocolVersion":1,"backendRevision":compatibility::BUZZ_REVISION}));Ok(())},
        ["inspect"]=>config::load().map(|c|println!("{}",serde_json::json!({"relay":c.relay,"identity":c.identity,"configured":c.relay.is_some()&&c.identity.is_some(),"helperVersion":env!("CARGO_PKG_VERSION"),"protocolVersion":1,"backendRevision":compatibility::BUZZ_REVISION}))),
        ["setup","relay",url]=>config::canonical_relay(url).and_then(|relay|config::save(&config::with_relay(config::load()?,relay))),
        ["setup","identity","enroll"]=>enrollment::enroll().await,
        _=>Err("usage: omarchy-buzz daemon [--keep-running] | ui-bridge | agents-daemon [--keep-running] | agents-bridge | inspect | --version | setup relay URL | setup identity enroll")
    }
}

fn main() {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("runtime_unavailable");
            std::process::exit(1);
        }
    };
    let result = runtime.block_on(run());
    // Secret Service operations cannot be forcibly cancelled. Keep shutdown
    // bounded even if a D-Bus/keyring operation remains on the blocking pool.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    if let Err(category) = result {
        eprintln!("{category}");
        std::process::exit(1);
    }
}
