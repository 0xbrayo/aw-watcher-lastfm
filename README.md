# Activitywatch watcher for last.fm
[![Build](https://github.com/brayo-pip/aw-watcher-lastfm/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/brayo-pip/aw-watcher-lastfm/actions/workflows/rust.yml) [![dependency status](https://deps.rs/repo/github/0xbrayo/aw-watcher-lastfm/status.svg)](https://deps.rs/repo/github/0xbrayo/aw-watcher-lastfm)

This is a simple activitywatch watcher for last.fm scrobble data. It uses the last.fm API to fetch scrobbles and sends them to the activitywatch server.

# Prerequisites

- [Activitywatch](https://github.com/ActivityWatch/activitywatch)
- [Rust](https://www.rust-lang.org/tools/install)
- [Last.fm API account](https://www.last.fm/)

# Installation

Download the binary for your OS from releases, or use one the the options below

## Installation using cargo

```bash
cargo install --git https://github.com/brayo-pip/aw-watcher-lastfm.git
```

You can then add `aw-watcher-lastfm` to `autostart_modules` in your `aw-qt.toml`

i.e:
```ini
[aw-qt]
autostart_modules = ["aw-server-rust","aw-awatcher", "aw-watcher-lastfm"]
```

## Installation from source

Clone the repository

```bash
git clone https://github.com/brayo-pip/aw-watcher-lastfm.git
```

cd into the directory

```bash
cd aw-watcher-lastfm
```


On first run, you will be prompted to configure last.fm API key and your last.fm username. You can get the apikey from the [Last.fm API page](https://www.last.fm/api/accounts).

```bash
cargo run
```

This should take a few seconds then the events should be visible in localhost:5600. If aw-server or aw-server-rust is running.

![image](https://github.com/brayo-pip/aw-watcher-lastfm/assets/62670517/1c4cb5ff-5f2d-455b-845b-a3fcd8200f94)



If everything works as expected, you can build the binary, add it to `aw-qt` or set up a systemd service to run it in the background(if running linux).

```bash
cargo build --release
```

# Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.
