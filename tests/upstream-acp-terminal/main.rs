// Only AcpClient is a test double. run_authenticate and terminal_auth are exact
// patched upstream code, including the real terminal subprocess lifecycle.
use std::{time::Duration,sync::atomic::{AtomicUsize,Ordering}};
type Result<T> = anyhow::Result<T>;
mod terminal_auth;
const MODELS_TIMEOUT:Duration=Duration::from_secs(1);
const AUTHENTICATE_TIMEOUT:Duration=Duration::from_millis(300);
struct AuthAgentArgs {agent_command:String,agent_args:Vec<String>}
struct AuthenticateArgs {agent:AuthAgentArgs,method_id:String}
mod config {
    pub fn normalize_agent_args(_: &str,args:Vec<String>)->Vec<String>{args}
}
static NEXT:AtomicUsize=AtomicUsize::new(0);
struct AcpClient {trace:String,case:String,index:usize}
fn record(path:&str,event:&str) {
    use std::io::Write;
    writeln!(std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap(),"{event}").unwrap();
}
async fn spawn_auth_client(agent:&AuthAgentArgs)->Result<AcpClient> {
    let trace=agent.agent_args[0].clone();record(&trace,"spawn");
    Ok(AcpClient {trace,case:agent.agent_args[1].clone(),index:NEXT.fetch_add(1,Ordering::SeqCst)})
}
impl AcpClient {
    async fn initialize_for_auth(&mut self,terminal:bool)->Result<serde_json::Value> {
        record(&self.trace,if terminal {"initialize-terminal"} else {"initialize-no-terminal"});
        if self.index>0 && self.case=="reconnect-fail" {return Err(anyhow::anyhow!("synthetic"));}
        let mut method=serde_json::json!({"id":"login","type":"terminal","args":["--login"],"env":{"ACP_INTERACTIVE_LOGIN":"1"}});
        if self.case=="agent" {method=serde_json::json!({"id":"login"});}
        if self.case=="command" {method["command"]="/bin/sh".into();}
        if self.case=="unsafe-env" {method["env"]=serde_json::json!({"LD_PRELOAD":"synthetic"});}
        if self.case=="duplicate" {return Ok(serde_json::json!({"authMethods":[method.clone(),method]}));}
        Ok(serde_json::json!({"authMethods":[method]}))
    }
    async fn authenticate(&mut self,_:&str)->Result<serde_json::Value>{record(&self.trace,"authenticate");Ok(serde_json::json!({}))}
    async fn shutdown(&mut self){record(&self.trace,"shutdown");}
}
fn extract_auth_methods(value:&serde_json::Value)->Vec<serde_json::Value>{value["authMethods"].as_array().cloned().unwrap_or_default()}
// STAGED_RUN_AUTHENTICATE
#[tokio::main(flavor="current_thread")]
async fn main() {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    let request=AuthenticateArgs {agent:AuthAgentArgs {agent_command:args[0].clone(),agent_args:vec![args[1].clone(),args[2].clone()]},method_id:"login".into()};
    let before=nix::sys::termios::tcgetattr(std::io::stdin()).ok();
    let result=run_authenticate(request).await;
    if let Some(before)=before {
        assert_eq!(nix::unistd::tcgetpgrp(std::io::stdin()).unwrap(),nix::unistd::getpgrp());
        assert_eq!(nix::sys::termios::tcgetattr(std::io::stdin()).unwrap(),before);
        record(&args[1],"terminal-restored");
    }
    record(&args[1],if result.is_ok(){"success"}else{"failure"});
    if let Err(error)=result {eprintln!("{error}");std::process::exit(1);}
}
