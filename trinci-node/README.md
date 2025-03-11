# TRINCI Node

This project is the Affidaty S.p.A. implementation of the TRINCI Node. TRINCI is a permissionless, anti-fork blockchain based on the PoP consensus algorithm.

## Installation Guide

### Pre-requisites

#### Supported OS
The project has been developed and tested in **Fedora** and **Ubuntu** OSs. Compatibility with other OSs is currently in testing phase and not guaranteed.

#### Dependencies
The TRINCI Node is developed in Rust. Please install the latest Rust version by following the [community guide](https://www.rust-lang.org/tools/install).

After installing Rust, ensure the following dependencies are resolved:
- Clang-devel: [link](https://clang.llvm.org/)
- Llvm-devel
- OpenSSL

### Installation

With the following steps, you can build the project:

1. Build the project: `cargo build --release [FEATURES]`
   In case of any features, use it as: `cargo build --release --features [FEATURE NAME]`.

#### Features

The TRINCI Node offers several features to add extra functionalities:

- `default`: it is the default feature, it uses the `standalone` feature, this setup is intended to be a standard TRINCI node that can be connected to a P2P TRINCI blockchain. This is probably the common user use case.
- `indexer`: this feature enables a module that traces any balance change on the blockchain wallets, the WASM machine collects the transactions outcomes and the indexer service sends those to a CouchDB server. 
- `kafka-producer`: this feature propagates any TRINCI Node event to a Kafka server.
- `mining`: it enables the interfaces used by th mining tool. The mining tool is an external project used to mine T-coin and join the players group. **Note**: by enabling this, the node exposes methods that can be exploited to move tokens from its wallet. 
- `playground`: this feature is used by the playground project. This mock-up some of the components of the node to simulate TRINCI blockchain network. It is used mainly to test P2P and consensus functionalities.  
- `standalone`: it is used by the `default` feature.

## Usage Guide

Below is the CLI guide for configuring and running the TRINCI Node.

### Command Line Interface (CLI)

Use the following parameters to configure the node:

- `-a`, `--autoreplica-peer-addr=<IP ADDR>:<PORT>`: address of a known TRINCI Node, the local node will connect to its network and align to its state (DB and peers). I.E.: `http://127.0.0.1:8080`
- `-b`, `--blockchain-bootstrap-file=<FILE PATH>`: path to a local bootstrap file
- `-d`, `--database-path=<DIRECTORY PATH>`: path to a local RocksDB directory
- `-k`, `--p2p-keypair-file=<FILE PATH>`: path to a keypair file in `pkcs8` format
- `-o`, `--p2p-port=<PORT>`: port for P2P service
- `-P`, `--p2p-bootstrap-peer-addresses=<MULTIADDR>`: address of a known P2P peer, the local node will gather its own P2P neighbors from it. Note: without a known P2P peer, the node will be excluded from the network. I.E.: `/ip4/198.51.100/tcp/1234`
- `-p`, `--p2p-bootstrap-peer-id=<ACCOUNT ID>`: account ID of a known P2P peer, the local node will gather its own P2P neighbors from it. Note: without a known P2P peer, the node will be excluded from the network. 
- `-R`, `--rest-address=<IP ADDR>`: exposed interface to serve the REST service (`0.0.0.0` to expose all the interfaces)
- `-r`, `--rest-port=<PORT>`: port for REST service
- `-S`, `--socket-address=<IP ADDR>`: exposed interface to serve the Socket service (`0.0.0.0` to expose all the interfaces) 
- `-s`, `--socket-port=<PORT>`: port for Socket service
- `-t`, `--trinci-keypair-file=<FILE PATH>`: path to a keypair file in `pkcs8` format
- `-h`, `--help`: print help
- `-V`, `--version`: print version


### CLI Example Usage

To start the TRINCI Node with a specific configuration, you can use the following example commands:

#### Default Mode
```sh
../target/release/node_standalone -b data/bs/bootstrap.bin -r 8780  -t data/kp/trinci_kp/keypair.kp 
```

#### With Indexer Feature
```sh
../target/release/node_standalone -b data/bs/bootstrap.bin -r 9090 -t data/kp/trinci_kp/keypair.kp --indexer-db-name trinci --indexer-host localhost --indexer-password password --indexer-port 5984 --indexer-user admin
```

#### With Kafka Producer Feature
```sh
../target/release/node_standalone --rest-address 127.0.0.1 --rest-port 8080 --database-path /path/to/db --kafka-host 127.0.0.1 --kafka-port 9092
```

### Additional Sections to Consider (TODO)

## REST API
The following are the REST endpoints to interact with the node

- `/` **[GET]**: It responds ok in case the node works properly
- `/get_account/{account_id}` **[GET]**: responds with the requested account
- `/get_account_data/{account_id}/{key}` **[GET]**: responds with the data to the given key and account
- `/get_full_block/{block_height}` **[GET]**: responds with the block corresponding with the requested height
- `/get_players` **[GET]**: responds with the player list
- `/get_receipt/{receipt_hash}` **[GET]**: responds with the receipt of the requested transaction identified by the hash
- `/system_status` **[GET]**: responds with the node system stats
- `/unconfirmed_tx_pool_len` **[GET]**: returns the local len of the unconfirmed pool
- `/status` **[GET]**: responds with the node configuration
- `/submit_tx` **[POST]**: endpoint to submit transactions via post
- `/visa` **[GET]**: responds with the node TRINCI infos 
- `/bootstrap` **[GET]**: serve de node bootstrap file

**Configuration File**

**Running in Docker**

**Environment Variables**

**Troubleshooting**

**Contact and Support**
