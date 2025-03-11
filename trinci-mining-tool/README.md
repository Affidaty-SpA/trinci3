# Trinci Mining Tool

## Overview

Trinci Mining Tool is a web application designed to monitor and control mining operations of T.R.I.N.C.I. blockchain nodes. The application provides functionalities to start and stop mining, monitor node statistics, and manage network nodes efficiently.

To permit the Mining Tool to work properly it is needed to enable the `mining` feature in the TRINCI Node.

## Features

- **Node Mining Control**: Start and stop mining processes of your Trinci nodes.
- **Node Monitoring**: Real-time monitoring of node performance and blockchain interactions.
- **Network Node Management**: Manage and configure network nodes.
- **REST and WebSocket Interfaces**: Seamless interaction with nodes via RESTful API and WebSocket connections.

## Installation

### Prerequisites

- Rust (latest stable version)
- Cargo (comes with Rust)

### Steps

#### Backend steps

1. Clone the repository:
    ```sh
    git clone TODO
    cd trinci-mining-tool
    ```

2. Build the project:
    ```sh
    cargo build --release
    ```

3. Run the project:
    ```sh
    cargo run --release
    ```

#### Frontend steps

TODO

## Usage

### CLI Options

The application provides several command-line options for configuration:

- `--keypair_path` (`-k`): Path to the keypair file. Default is `mt.kp`.
- `--rest_addr` (`-r`): Address for the REST interface. Default is `0.0.0.0:7000`.
- `--socket_addr` (`-s`): Address for the WebSocket interface. Default is `0.0.0.0:5000`.

Example:

```sh
trinci-mining-tool --keypair_path path/to/your/keypair.kp --rest_addr 0.0.0.0:8000 --socket_addr 0.0.0.0:6000
```

### REST API Endpoints

- `POST /mining/identity`: Retrieve node identity information.
- `GET /mining/refresh`: Refresh node data.
- `POST /mining/rpc`: Execute RPC commands.
- `GET /mining/scan`: Scan the network for other nodes.
- `POST /mining/manual_add_node`: Manually add a node to the network.

### WebSocket Setup

The application sets up a WebSocket interface that allows for real-time communication with the frontend for effective monitoring and control.

## Mining Life Cycle

The following describes the steps a proto-miner needs to follow to become a player in the Trinci blockchain:

1. **Compile the Nodes with Mining Feature:**
   Ensure the nodes are compiled with the mining feature enabled.

2. **Start the Nodes and Discover Account IDs:**
   Start the nodes and discover their account IDs.

3. **Await Network Synchronization:**
   Wait for the nodes to sync with the blockchain network.

4. **Purchase Minimum BITBEL:**
   Obtain BITBEL by sending EURS to the fuel machine, and then send BITBEL to the nodes' accounts.

5. **Start Mining Process:**
   Use the mining tool to initiate mining.


Successfully complete a mining phase to obtain STAKE_COIN.
The nodes automatically become players in the network.
A player node is randomly selected to receive fuel rewards based on the stake of mined and transferred STAKE_COIN.
Use the mining tool to transfer BITBEL to any account, including the fuel machine for exchanging back to EURS.
The node restarts the mining process automatically.

## Contact

For any inquiries or support, feel free to open an issue on GitHub or contact the repository owner.
