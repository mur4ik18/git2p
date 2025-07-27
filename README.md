# git2p

P2P git-like file manager.

## Description

git2p is a CLI application for synchronizing files between computers directly (peer-to-peer), with a custom change history journal and the ability to revert changes. The project is designed for learning how git and p2p networks work.

## Features
- P2P connection between computers
- Custom change history journal (git-like, but simpler)
- Optional automatic synchronization when new devices join
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
- [x] File change watching
- [x] P2P connection between two computers (`connect`)
- [x] Synchronize changes (`sync`)
- [x] Colorful CLI interface

### Future
- [ ] Automatic sync when new devices join
- [ ] Conflict resolver for simultaneous changes
- [ ] Mobile app
- [ ] Web interface

## Progress

- [x] MVP: 100%
