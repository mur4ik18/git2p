use clap::{Parser, Subcommand};
use cliclack::{outro, spinner};
use futures::StreamExt;
use libp2p::{
    floodsub::{self, Floodsub, FloodsubEvent},
    identity,
    mdns,
    swarm::{NetworkBehaviour, SwarmEvent},
    PeerId,
};
use std::error::Error;
use std::fs;
use std::path::Path;
use tokio::io::AsyncBufReadExt;

#[derive(Parser)]
#[command(name = "git2p")]
#[command(about = "P2P git-like file manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// The NetworkBehaviour derives from libp2p's NetworkBehaviour macro.
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MyBehaviourEvent")]
struct MyBehaviour {
    floodsub: Floodsub,
    mdns: mdns::tokio::Behaviour,
}

#[allow(clippy::large_enum_variant)]
enum MyBehaviourEvent {
    Floodsub(FloodsubEvent),
    Mdns(mdns::Event),
}

impl From<FloodsubEvent> for MyBehaviourEvent {
    fn from(event: FloodsubEvent) -> Self {
        MyBehaviourEvent::Floodsub(event)
    }
}

impl From<mdns::Event> for MyBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        MyBehaviourEvent::Mdns(event)
    }
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Add {
        #[arg(required = true)]
        files: Vec<String>,
    },
    List,
    Rm {
        #[arg(required = true)]
        files: Vec<String>,
    },
    P2p,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::P2p => {
            let id_keys = identity::Keypair::generate_ed25519();
            let local_peer_id = PeerId::from(id_keys.public());
            println!("Local peer id: {local_peer_id}");

            let mut swarm = libp2p::SwarmBuilder::with_existing_identity(id_keys)
                .with_tokio()
                .with_tcp(
                    Default::default(),
                    libp2p::noise::Config::new,
                    libp2p::yamux::Config::default,
                )?
                .with_behaviour(|key| {
                    let local_peer_id = key.public().to_peer_id();
                    MyBehaviour {
                        floodsub: Floodsub::new(local_peer_id),
                        mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id).unwrap(),
                    }
                })?
                .with_swarm_config(|c| c.with_idle_connection_timeout(std::time::Duration::from_secs(30)))
                .build();

            // Create a Floodsub topic
            let floodsub_topic = floodsub::Topic::new("chat");
            swarm
                .behaviour_mut()
                .floodsub
                .subscribe(floodsub_topic.clone());

            let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

            swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

            loop {
                tokio::select! {
                    line = stdin.next_line() => {
                        let line = line?.expect("stdin closed");
                        swarm.behaviour_mut().floodsub.publish(floodsub_topic.clone(), line);
                    }
                    event = swarm.select_next_some() => {
                        match event {
                            SwarmEvent::NewListenAddr { address, .. } => {
                                println!("Listening on {address}");
                            }
                            SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(event)) => {
                                match event {
                                    mdns::Event::Discovered(list) => {
                                        for (peer, _) in list {
                                            swarm.behaviour_mut().floodsub.add_node_to_partial_view(peer);
                                        }
                                    }
                                    mdns::Event::Expired(list) => {
                                        for (peer, _) in list {
                                            if !swarm.behaviour().mdns.discovered_nodes().any(|p| p == &peer) {
                                                swarm.behaviour_mut().floodsub.remove_node_from_partial_view(&peer);
                                            }
                                        }
                                    }
                                }
                            }
                            SwarmEvent::Behaviour(MyBehaviourEvent::Floodsub(event)) => {
                                if let FloodsubEvent::Message(message) = event {
                                    println!(
                                        "Received: '{:?}' from {:?}",
                                        String::from_utf8_lossy(&message.data),
                                        message.source
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Commands::Init => {
            let sp = spinner();
            sp.start("Repository initialization...");

            let repo_path = Path::new(".git2p");

            if repo_path.exists() {
                sp.stop("Repository already initialized!");
            } else {
                match fs::create_dir(repo_path) {
                    Ok(_) => {
                        sp.stop("Repository initialized!");
                    }
                    Err(e) => {
                        sp.error(&format!("Failed to initialize repository: {e}"));
                        return Ok(());
                    }
                }
            }

            let _ = outro("You can now add files to tracking.");
        }
        Commands::Add { files } => {
            let sp = spinner();
            sp.start("Adding files...");

            let repo_path = Path::new(".git2p");
            if !repo_path.exists() {
                sp.error("Repository not initialized! Run 'git2p init' first.");
                return Ok(());
            }

            for file in files {
                let file_path = Path::new(file);
                if !file_path.exists() {
                    sp.error(&format!("File '{file}' not found!"));
                    continue;
                }

                let dest_path = repo_path.join(file_path.file_name().unwrap());
                match fs::copy(file_path, dest_path) {
                    Ok(_) => {
                        sp.set_message(&format!("Added '{file}'"));
                    }
                    Err(e) => {
                        sp.error(&format!("Failed to add '{file}': {e}"));
                    }
                }
            }

            sp.stop("Done.");
        }
        Commands::List => {
            let repo_path = Path::new(".git2p");
            if !repo_path.exists() {
                let _ = cliclack::outro("Error: Repository not initialized! Run 'git2p init' first.");
                return Ok(());
            }

            let entries = match fs::read_dir(repo_path) {
                Ok(entries) => entries,
                Err(e) => {
                    let _ = cliclack::outro(format!("Error: Failed to read repository: {e}"));
                    return Ok(());
                }
            };

            let files: Vec<String> = entries
                .filter_map(|entry| {
                    entry.ok().and_then(|e| {
                        e.path()
                            .file_name()
                            .and_then(|n| n.to_str().map(String::from))
                    })
                })
                .collect();

            if files.is_empty() {
                let _ = cliclack::outro("No files added yet.");
            } else {
                let _ = cliclack::outro(format!("Tracked files:\n{}", files.join("\n")));
            }
        }
        Commands::Rm { files } => {
            let sp = spinner();
            sp.start("Removing files...");

            let repo_path = Path::new(".git2p");
            if !repo_path.exists() {
                sp.error("Repository not initialized! Run 'git2p init' first.");
                return Ok(());
            }

            for file in files {
                let file_path = repo_path.join(file);
                if !file_path.exists() {
                    sp.error(&format!("File '{file}' not found in repository!"));
                    continue;
                }

                match fs::remove_file(file_path) {
                    Ok(_) => {
                        sp.set_message(&format!("Removed '{file}'"));
                    }
                    Err(e) => {
                        sp.error(&format!("Failed to remove '{file}': {e}"));
                    }
                }
            }
            sp.stop("Done.");
        }
    }
    Ok(())
}
