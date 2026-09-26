#![forbid(unsafe_code)]
mod auth;
mod config;
mod ipc;
mod protocol;
// M1 transport seam is intentionally not exposed through UI IPC.
#[allow(dead_code)]
mod query;
use std::io::IsTerminal;
fn enroll() -> Result<(), &'static str> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err("terminal_required");
    }
    let mut c = config::load()?;
    if c.relay.is_none() {
        return Err("unconfigured");
    }
    eprintln!("Enroll an existing Buzz identity in this helper's own Secret Service namespace.");
    let secret = zeroize::Zeroizing::new(
        rpassword::prompt_password("Private key (hidden): ").map_err(|_| "terminal_unavailable")?,
    );
    let keys = nostr::Keys::parse(secret.as_str()).map_err(|_| "identity_invalid")?;
    c.identity = Some(keys.public_key().to_hex());
    let entry = keyring::Entry::new(auth::SERVICE, &config::account(&c)?)
        .map_err(|_| "identity_unavailable")?;
    entry
        .set_password(secret.as_str())
        .map_err(|_| "identity_unavailable")?;
    config::save(&c)?;
    println!(
        "{}",
        serde_json::json!({"identity":c.identity,"relay":c.relay,"status":"enrolled"})
    );
    Ok(())
}
async fn run() -> Result<(), &'static str> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice(){
        ["daemon"]=>ipc::daemon(false).await,
        ["daemon","--keep-running"]=>ipc::daemon(true).await,
        ["ui-bridge"]=>ipc::bridge().await,
        ["inspect"]=>config::load().map(|c|println!("{}",serde_json::json!({"relay":c.relay,"identity":c.identity,"configured":c.relay.is_some()&&c.identity.is_some(),"protocolVersion":1,"backendRevision":"781d39510cf23cfe224e8f521ae06a23377e06de"}))),
        ["setup","relay",url]=>config::canonical_relay(url).and_then(|relay|{let mut c=config::load()?;if c.relay.as_deref()!=Some(&relay){c.identity=None;}c.relay=Some(relay);config::save(&c)}),
        ["setup","identity","enroll"]=>tokio::task::spawn_blocking(enroll).await.unwrap_or(Err("identity_unavailable")),
        _=>Err("usage: omarchy-buzz daemon [--keep-running] | ui-bridge | inspect | setup relay URL | setup identity enroll")
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
