//! `ve`: the CLI. Each subcommand maps to one protocol method.
//!
//! For now calls are dispatched in-process against the same registry the daemon
//! serves: the CLI opens the project (taking its lock), runs the call and
//! exits. Once ve-daemon has its socket, this becomes connect-or-spawn.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use ve_protocol::{ErrorCode, Registry};

#[derive(Parser)]
#[command(name = "ve", version, about = "Headless, agent-friendly video engine")]
struct Cli {
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    json: bool,
    /// Project folder. Defaults to the nearest folder at or above the current
    /// directory that holds a project.otio.
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Engine, protocol and schema versions.
    Version,
    /// Print the OpenRPC document describing every method.
    Describe,
    /// Create a project folder: `ve init my-edit --fps 30 --size 1920x1080`.
    Init {
        path: PathBuf,
        /// Frame rate: 30, 29.97, 23.976 or an exact 30000/1001.
        #[arg(long)]
        fps: String,
        /// Output size, WIDTHxHEIGHT.
        #[arg(long)]
        size: String,
        /// Defaults to the folder name.
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
    },
    /// Compact timeline text: tracks, clips and markers with their IDs.
    Summary,
    /// Send a raw JSON-RPC method call, e.g. `ve call project.get '{}'`.
    Call { method: String, params: Option<String> },
}

/// A failed call: the JSON-RPC error object.
struct CallError(Value);

async fn call(registry: &Registry, method: &str, params: Value) -> Result<Value, CallError> {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let resp: Value = serde_json::from_str(&registry.dispatch(&req).await)
        .unwrap_or_else(|e| json!({ "error": { "code": -32603, "message": e.to_string() } }));
    match resp.get("error") {
        Some(err) => Err(CallError(err.clone())),
        None => Ok(resp["result"].clone()),
    }
}

fn fail(err: &Value, json_output: bool) -> ! {
    if json_output {
        eprintln!("{}", serde_json::to_string_pretty(err).unwrap_or_default());
    } else {
        eprintln!("error: {}", err["message"].as_str().unwrap_or("unknown error"));
    }
    let code = err["code"].as_i64().unwrap_or(0);
    std::process::exit(ErrorCode::from_rpc_code(code).exit_code());
}

fn usage_error(message: String, json_output: bool) -> ! {
    fail(&json!({ "code": ErrorCode::InvalidParams.rpc_code(), "message": message }), json_output)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let registry = ve_daemon::build_registry();

    // Open the project first for commands that work on one.
    let needs_project = match &cli.command {
        Command::Summary => Some(true),
        Command::Call { method, .. } if !method.starts_with("system.") && method != "project.init" && method != "project.open" => {
            Some(false)
        }
        _ => None,
    };
    if let Some(required) = needs_project {
        let cwd = std::env::current_dir()?;
        let root = cli.project.clone().or_else(|| ve_daemon::project::find_root(&cwd));
        match root {
            Some(path) => {
                if let Err(CallError(e)) = call(&registry, "project.open", json!({ "path": path })).await {
                    fail(&e, cli.json);
                }
            }
            None if required => usage_error(
                format!("no project.otio in {} or any parent folder; pass --project or run `ve init`", cwd.display()),
                cli.json,
            ),
            None => {}
        }
    }

    let (method, params) = match &cli.command {
        Command::Version => ("system.version".to_owned(), json!({})),
        Command::Describe => ("system.describe".to_owned(), json!({})),
        Command::Init { path, fps, size, name, sample_rate } => {
            let Some((w, h)) = size.split_once(['x', 'X']).and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
            else {
                usage_error(format!("--size `{size}`: use WIDTHxHEIGHT, e.g. 1920x1080"), cli.json)
            };
            let params = json!({ "path": path, "name": name, "fps": fps, "width": w, "height": h, "sample_rate": sample_rate });
            ("project.init".to_owned(), params)
        }
        Command::Summary => ("project.summary".to_owned(), json!({})),
        Command::Call { method, params } => {
            let p = match params.as_deref().map(serde_json::from_str).transpose() {
                Ok(p) => p.unwrap_or(json!({})),
                Err(e) => usage_error(format!("params are not valid JSON: {e}"), cli.json),
            };
            (method.clone(), p)
        }
    };

    let result = match call(&registry, &method, params).await {
        Ok(r) => r,
        Err(CallError(e)) => fail(&e, cli.json),
    };

    match (&cli.command, cli.json) {
        (Command::Version, false) => println!(
            "ve {}  protocol {}  schema {}",
            result["engine"].as_str().unwrap_or("?"),
            result["protocol"],
            result["schema"].as_str().unwrap_or("?")
        ),
        (Command::Init { .. }, false) => println!(
            "created {} ({})",
            result["path"].as_str().unwrap_or("?"),
            result["name"].as_str().unwrap_or("?")
        ),
        (Command::Summary, false) => print!("{}", result["text"].as_str().unwrap_or_default()),
        _ => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}
