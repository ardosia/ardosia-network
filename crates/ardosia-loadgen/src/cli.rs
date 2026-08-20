use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ardosia-loadgen")]
#[command(about = "RakNet transport load generator for Ardosia")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Local {
        scenario: PathBuf,
        #[arg(long, default_value = "127.0.0.1:19132")]
        bind: SocketAddr,
        #[arg(long)]
        worker_shards: Option<usize>,
    },
    Profile {
        scenario: PathBuf,
        #[arg(long, default_value = "127.0.0.1:19132")]
        bind: SocketAddr,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        worker_shards: Option<usize>,
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
