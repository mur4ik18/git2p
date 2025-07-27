use chrono::Utc;
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
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;
use tokio::io::AsyncBufReadExt;
use notify::{RecursiveMode, Watcher};

#[derive(Serialize, Deserialize, Debug)]
struct Commit {
    id: String,
    message: String,
    timestamp: String,
}

#[derive(Serialize, Deserialize, Debug)]
enum SyncMessage {
    AskForCommits,
    MyCommits { commits: Vec<String> },
}

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
    Commit {
        #[arg(short, long)]
        message: String,
    },
    Log,
    Watch,
    Revert {
        #[arg(required = true)]
        commit_id: String,
    },
    Connect {
        #[arg(long)]
        addr: Option<String>,
    },
    List,
    Rm {
        #[arg(required = true)]
        files: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Connect { addr } => {
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
                        mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)
                            .unwrap(),
                    }
                })?
                .with_swarm_config(|c| {
                    c.with_idle_connection_timeout(std::time::Duration::from_secs(30))
                })
                .build();

            // Create a Floodsub topic
            let floodsub_topic = floodsub::Topic::new("chat");
            swarm
                .behaviour_mut()
                .floodsub
                .subscribe(floodsub_topic.clone());

            if let Some(addr_str) = addr {
                let remote: libp2p::Multiaddr = addr_str.parse()?;
                swarm.dial(remote)?;
                println!("Dialed peer at {addr_str}");
            }

            let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

            swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
            println!("Type 'sync' to exchange commit data with peers.");

            loop {
                tokio::select! {
                    line = stdin.next_line() => {
                        let line = line?.expect("stdin closed");
                        if line.trim() == "sync" {
                            let message = SyncMessage::AskForCommits;
                            let json = serde_json::to_string(&message)?;
                            swarm.behaviour_mut().floodsub.publish(floodsub_topic.clone(), json);
                            println!("Published 'AskForCommits' to peers.");
                        } else {
                             swarm.behaviour_mut().floodsub.publish(floodsub_topic.clone(), line);
                        }

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
                                     if let Ok(sync_message) = serde_json::from_slice::<SyncMessage>(&message.data) {
                                        match sync_message {
                                            SyncMessage::AskForCommits => {
                                                println!("Received AskForCommits from {:?}", message.source);
                                                let local_commits = get_local_commits()?;
                                                let response = SyncMessage::MyCommits { commits: local_commits };
                                                let json = serde_json::to_string(&response)?;
                                                swarm.behaviour_mut().floodsub.publish(floodsub_topic.clone(), json);
                                            }
                                            SyncMessage::MyCommits { commits } => {
                                                println!("Received MyCommits from {:?}", message.source);
                                                let local_commits = get_local_commits()?;
                                                let new_commits: Vec<_> = commits.into_iter().filter(|c| !local_commits.contains(c)).collect();
                                                if !new_commits.is_empty() {
                                                    println!("New remote commits found: {:?}", new_commits);
                                                } else {
                                                    println!("You are up to date with peer {:?}.", message.source);
                                                }
                                            }
                                        }
                                    } else {
                                        println!(
                                            "Received: '{:?}' from {:?}",
                                            String::from_utf8_lossy(&message.data),
                                            message.source
                                        );
                                    }
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
        Commands::Commit { message } => {
            let sp = spinner();
            sp.start("Committing files...");

            let repo_path = Path::new(".git2p");
            if !repo_path.exists() {
                sp.error("Repository not initialized! Run 'git2p init' first.");
                return Ok(());
            }

            let versions_path = repo_path.join("versions");
            let logs_path = repo_path.join("logs");

            if !versions_path.exists() {
                fs::create_dir(&versions_path)?;
            }
            if !logs_path.exists() {
                fs::create_dir(&logs_path)?;
            }

            let timestamp = Utc::now().to_rfc3339();
            let mut hasher = Sha1::new();
            hasher.update(message.as_bytes());
            hasher.update(timestamp.as_bytes());
            let commit_id = format!("{:x}", hasher.finalize());
            let short_commit_id = &commit_id[0..7];

            let commit = Commit {
                id: short_commit_id.to_string(),
                message: message.clone(),
                timestamp: timestamp.clone(),
            };

            let commit_dir = versions_path.join(short_commit_id);
            fs::create_dir(&commit_dir)?;

            let tracked_files = fs::read_dir(repo_path)?
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_file())
                .map(|entry| entry.path())
                .collect::<Vec<_>>();

            for file_path in tracked_files {
                let dest_path = commit_dir.join(file_path.file_name().unwrap());
                fs::copy(&file_path, &dest_path)?;
            }

            let log_file_path = logs_path.join(format!("{}.json", short_commit_id));
            let mut log_file = fs::File::create(log_file_path)?;
            log_file.write_all(serde_json::to_string_pretty(&commit)?.as_bytes())?;

            sp.stop(format!("Committed with id: {short_commit_id}"));
        }
        Commands::Log => {
            let repo_path = Path::new(".git2p");
            let logs_path = repo_path.join("logs");

            if !logs_path.exists() {
                let _ = cliclack::outro("No commits yet.");
                return Ok(());
            }

            let mut commits: Vec<Commit> = fs::read_dir(logs_path)?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.is_file() && path.extension()? == "json" {
                        let content = fs::read_to_string(path).ok()?;
                        serde_json::from_str(&content).ok()
                    } else {
                        None
                    }
                })
                .collect();
            
            commits.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

            if commits.is_empty() {
                let _ = cliclack::outro("No commits yet.");
            } else {
                for commit in commits {
                    let _ = cliclack::outro(format!(
                        "commit {}\nAuthor: {}\nDate:   {}\n\n\t{}",
                        commit.id, "User", commit.timestamp, commit.message
                    ));
                }
            }
        }
        Commands::Watch => {
            let sp = spinner();
            sp.start("Watching for file changes...");

            let repo_path = Path::new(".git2p");
            if !repo_path.exists() {
                sp.error("Repository not initialized! Run 'git2p init' first.");
                return Ok(());
            }

            let tracked_files: Vec<String> = fs::read_dir(repo_path)
                .unwrap()
                .filter_map(|entry| {
                    let path = entry.ok()?.path();
                    if path.is_file() {
                        path.file_name()
                            .and_then(|n| n.to_str().map(String::from))
                    } else {
                        None
                    }
                })
                .collect();

            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = notify::recommended_watcher(tx)?;

            for file in &tracked_files {
                watcher.watch(Path::new(file), RecursiveMode::NonRecursive)?;
            }
            
            sp.stop("Now watching for changes. Press Ctrl+C to stop.");

            for res in rx {
                match res {
                    Ok(event) => {
                        if let notify::EventKind::Modify(_) = event.kind {
                             let _ = cliclack::outro(format!("File modified: {:?}", event.paths));
                        }
                    }
                    Err(e) => {
                        let _ = cliclack::outro(format!("watch error: {:?}", e));
                    }
                }
            }
        }
        Commands::Revert { commit_id } => {
            let sp = spinner();
            sp.start(format!("Reverting to commit {}...", commit_id));

            let repo_path = Path::new(".git2p");
            if !repo_path.exists() {
                sp.error("Repository not initialized! Run 'git2p init' first.");
                return Ok(());
            }

            let versions_path = repo_path.join("versions");
            let commit_path = versions_path.join(&commit_id);

            if !commit_path.exists() {
                sp.error(format!("Commit with id '{}' not found.", commit_id));
                return Ok(());
            }

            let files_to_revert = fs::read_dir(&commit_path)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .collect::<Vec<_>>();

            for file_path in files_to_revert {
                let file_name = file_path.file_name().unwrap();
                let dest_path = Path::new(".").join(file_name);
                fs::copy(&file_path, &dest_path)?;
                sp.set_message(format!("Reverted '{}'", file_name.to_str().unwrap()));
            }

            sp.stop(format!("Successfully reverted to commit {}.", commit_id));
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

            let tracked_files: Vec<String> = entries
                .filter_map(|entry| {
                    let path = entry.ok()?.path();
                    if path.is_file() {
                        path.file_name()
                            .and_then(|n| n.to_str().map(String::from))
                    } else {
                        None
                    }
                })
                .collect();

            if tracked_files.is_empty() {
                let _ = cliclack::outro("No files added yet.");
            } else {
                let _ = cliclack::outro(format!("Tracked files:\n{}", tracked_files.join("\n")));
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

fn get_local_commits() -> Result<Vec<String>, Box<dyn Error>> {
    let repo_path = Path::new(".git2p");
    let logs_path = repo_path.join("logs");

    if !logs_path.exists() {
        return Ok(Vec::new());
    }

    let commits = fs::read_dir(logs_path)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some() && path.extension().unwrap() == "json" {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(String::from)
            } else {
                None
            }
        })
        .collect();
    Ok(commits)
}
