# Mostro CLI 🧌

![Mostro-logo](static/logo.png)

Very simple command line interface that show all new replaceable events from [Mostro](https://github.com/MostroP2P/mostro)

## Requirements:

0. You need Rust version 1.64 or higher to compile.
1. You will need a lightning network node

## Install dependencies:

To compile on Ubuntu/Pop!\_OS, please install [cargo](https://www.rust-lang.org/tools/install), then run the following commands:

```
$ sudo apt update
$ sudo apt install -y cmake build-essential pkg-config
```

## Install

To install you need to fill the env vars (`.env`) on the with your own private key and add a Mostro pubkey.

```
$ git clone https://github.com/MostroP2P/mostro-cli.git
$ cd mostro-cli
$ cp .env-sample .env
$ cargo run
```
