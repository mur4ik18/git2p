# git2p

P2P git-like file manager.

## Description

git2p is a CLI application for synchronizing files between computers directly (peer-to-peer), with a custom change history journal and the ability to revert changes. The project is designed for learning how git and p2p networks work.

## How to Use

### Compiling the application
First, compile the project:
```bash
cargo build
```
All subsequent commands should be run from your project's directory using `./target/debug/git2p <command>`.

### Local Usage

1.  **Initialize a repository:**
    In your project folder, run the initialization command. This will create a `.git2p` directory to store versions and logs.
    ```bash
    ./target/debug/git2p init
    ```

2.  **Add files for tracking:**
    Add the files you want to track.
    ```bash
    ./target/debug/git2p add file1.txt file2.txt
    ```

3.  **Commit your changes:**
    Save the state of your tracked files by creating a commit with a message.
    ```bash
    ./target/debug/git2p commit -m "Your commit message"
    ```

4.  **View commit history:**
    To see the list of all commits, use the `log` command.
    ```bash
    ./target/debug/git2p log
    ```

5.  **Revert to a previous version:**
    You can restore the state of your files from a specific commit using its ID.
    ```bash
    ./target/debug/git2p revert <commit_id>
    ```

### P2P Synchronization

git2p allows you to synchronize your repository with other peers on the same network. It also automatically remembers peers you've successfully connected to, saving them in a `.git2p/known_peers.json` file. On startup, and periodically every 30 seconds, it will attempt to reconnect to these known peers to maintain synchronization.

1.  **Start a node:**
    On the first computer (e.g., in `peer1` directory), run the `connect` command. It will start listening for incoming connections and print its peer ID and listening address.
    ```bash
    # On Peer 1
    ./target/debug/git2p connect
    ```

2.  **Connect from another peer:**
    On the second computer (e.g., in `peer2` directory), run `connect` and provide the address of the first peer. You can also run it without an address to discover peers automatically via mDNS.
    ```bash
    # On Peer 2 (to connect to a specific address)
    ./target/debug/git2p connect --addr /ip4/192.168.1.5/tcp/56789

    # On Peer 2 (for automatic discovery)
    ./target/debug/git2p connect
    ```
    Once connected, the peers will automatically exchange commit information.

3.  **Pull changes:**
    After making and committing changes on one peer, go to the other peer and run the `pull` command. This will fetch the latest commit and update your local files.
    ```bash
    # On the peer that needs to receive changes
    ./target/debug/git2p pull
    ```

## Commands

*   `init`: Initializes a new git2p repository.
*   `add <files...>`: Adds one or more files to tracking.
*   `rm <files...>`: Removes one or more files from tracking.
*   `commit -m <message>`: Records changes to the repository.
*   `log`: Shows the commit history.
*   `list`: Lists all tracked files.
*   `revert <commit_id>`: Reverts the working directory to a specific commit.
*   `watch`: Watches for changes in tracked files.
*   `connect [--addr <multiaddr>]`: Connects to the P2P network. Can optionally dial a specific peer address.
*   `pull`: Fetches the latest commit from the network and applies it to the working directory.

## Features
- P2P connection between computers
- Custom change history journal (git-like, but simpler)
- Automatic peer discovery and reconnection
- Beautiful CLI interface
- Ability to revert changes
- (Future) Mobile app for file access

## Roadmap

### MVP
- [x] Repository initialization (`init`)
- [x] Add files to tracking (`add`)
- [x] Commit changes (`commit`)
- [x] View change history (`log`)
- [x] Revert to previous version (`revert`)
- [x] File change watching (`watch`)
- [x] P2P connection between two computers (`connect`)
- [x] Synchronize changes (`pull`)
- [x] Colorful CLI interface

### Future
- [x] Automatic reconnection to known peers
- [ ] Transfer file diffs during synchronization
- [ ] Conflict resolver for simultaneous changes
- [ ] Mobile app
- [ ] Web interface

## Progress

- [x] MVP: 100%
