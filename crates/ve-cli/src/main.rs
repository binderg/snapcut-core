//! `ve`: the CLI. Each subcommand maps to one protocol method.
//!
//! For now calls are dispatched in-process against the same registry the daemon
//! serves. Once ve-daemon has its socket, this becomes connect-or-spawn.

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(name = "ve", version, about = "Headless, agent-friendly video engine")]
struct Cli {
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Engine, protocol and schema versions.
    Version,
    /// Print the OpenRPC document describing every method.
    Describe,
    /// Send a raw JSON-RPC method call, e.g. `ve call system.version '{}'`.
    Call { method: String, params: Option<String> },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let (method, params) = match &cli.command {
        Command::Version => ("system.version".to_string(), json!({})),
        Command::Describe => ("system.describe".to_string(), json!({})),
        Command::Call { method, params } => {
            let p = params.as_deref().map(serde_json::from_str).transpose()?.unwrap_or(json!({}));
            (method.clone(), p)
        }
    };

    let registry = ve_daemon::build_registry();
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let resp: Value = serde_json::from_str(&registry.dispatch(&req).await)?;

    if let Some(err) = resp.get("error") {
        eprintln!("{}", serde_json::to_string_pretty(err)?);
        let code = err["code"].as_i64().unwrap_or(0);
        std::process::exit(ve_protocol::ErrorCode::from_rpc_code(code).exit_code());
    }

    let result = &resp["result"];
    match (&cli.command, cli.json) {
        (Command::Version, false) => println!(
            "ve {}  protocol {}  schema {}",
            result["engine"].as_str().unwrap_or("?"),
            result["protocol"],
            result["schema"].as_str().unwrap_or("?")
        ),
        _ => println!("{}", serde_json::to_string_pretty(result)?),
    }
    Ok(())
}
