use clap::{Parser, Subcommand};
use cliclack::{outro, spinner};


#[derive(Parser)]
#[command(name="git2p")]
#[command(about="P2P git-like file manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            let sp = spinner();
            sp.start("Repository initialization...");

            std::thread::sleep(std::time::Duration::from_secs(1));

            sp.stop("Repository initialized!");
            outro("You can now add files to tracking.");
        }

    }
}
