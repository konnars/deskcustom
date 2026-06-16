use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use deskcustom_config::{Config, Role};
use deskcustom_core::ServiceHandle;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "deskcustom", about = "Keyboard & mouse sharing with fine-tuned debug")]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Server,
    Client {
        #[arg(long)]
        connect: Option<String>,
    },
    Check,
    /// Launch desktop app (alias — use Deskcustom.app after build)
    App,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Some(Command::App)) {
        eprintln!("Запусти Deskcustom.app или: cd apps/desktop && cargo tauri dev");
        return Ok(());
    }

    init_tracing();

    let mut config = if let Some(path) = &cli.config {
        Config::load_path(path).context("load config")?
    } else if Config::config_path().exists() {
        Config::load_or_default()
    } else {
        Config::default_with_screens()
    };

    match cli.command {
        Some(Command::Server) => {
            config.role = Role::Server;
            let handle = ServiceHandle::start(config)?;
            tokio::signal::ctrl_c().await?;
            handle.stop().await;
        }
        Some(Command::Client { connect }) => {
            config.role = Role::Client;
            if let Some(addr) = connect {
                config.client.server_addr = addr;
            }
            let handle = ServiceHandle::start(config)?;
            tokio::signal::ctrl_c().await?;
            handle.stop().await;
        }
        Some(Command::Check) => {
            println!("{}", toml::to_string_pretty(&config)?);
        }
        Some(Command::App) => unreachable!(),
        None => {
            let handle = ServiceHandle::start(config)?;
            tokio::signal::ctrl_c().await?;
            handle.stop().await;
        }
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
