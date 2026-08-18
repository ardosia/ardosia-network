use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ardosia_loadgen::child_protocol::run_stdio_child;
use ardosia_loadgen::report::RunReport;
use ardosia_loadgen::runner::{run_clients, run_local, serve_until};
use ardosia_loadgen::scenario::Scenario;
use clap::{Parser, Subcommand};
use tokio::sync::watch;

#[derive(Debug, Parser)]
#[command(name = "ardosia-loadgen")]
#[command(about = "RakNet transport load generator for Ardosia")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Local {
        scenario: PathBuf,
        #[arg(long, default_value = "127.0.0.1:19132")]
        bind: SocketAddr,
    },
    Run {
        scenario: PathBuf,
        #[arg(long)]
        target: SocketAddr,
    },
    Serve {
        #[arg(long, default_value = "0.0.0.0:19132")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 8)]
        protocol: u8,
        #[arg(long, default_value_t = 1024)]
        max_connections: usize,
    },
    #[command(hide = true)]
    ServeChild,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Local { scenario, bind } => {
            let scenario = load_scenario(&scenario)?;
            let report = run_local(bind, &scenario).await?;
            emit_report(&report)?;
        }
        Command::Run { scenario, target } => {
            let scenario = load_scenario(&scenario)?;
            let report = run_clients(target, &scenario).await;
            emit_report(&report)?;
        }
        Command::Serve {
            bind,
            protocol,
            max_connections,
        } => {
            let (stop_tx, stop_rx) = watch::channel(false);
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                let _ = stop_tx.send(true);
            });

            let metrics = serve_until(bind, protocol, max_connections, stop_rx).await?;
            eprintln!("server stopped: {metrics:?}");
        }
        Command::ServeChild => run_stdio_child().await?,
    }

    Ok(())
}

fn load_scenario(path: &Path) -> Result<Scenario, Box<dyn std::error::Error>> {
    let input = std::fs::read_to_string(path)?;
    Ok(Scenario::from_str(&input)?)
}

fn emit_report(report: &RunReport) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(report)?);
    if !report.passed {
        std::process::exit(1);
    }
    Ok(())
}
