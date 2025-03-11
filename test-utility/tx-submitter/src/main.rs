mod tx_builder;

use std::{time::Duration, vec};

use anyhow::Result;
use clap::Parser;
use log::{info, LevelFilter};
use serde_json::from_slice;
use tide::{Request, Response, Result as TideResult, StatusCode};
use trinci_core_new::{log_error, utils::init_logger};
use tx_builder::{craft_init_mining_sc, publish_sc, transfer_bb_tx, ContractRegistrationArgs};

use crate::tx_builder::mint_bb_tx;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(long, default_value = "http://127.0.0.1:9001")]
    pub local_rest_addr: String,
    #[arg(long, default_value = "http://127.0.0.1:9000")]
    pub node_rest_addr: String,
}

async fn submit_mint(req: Request<String>) -> TideResult {
    let target_user = match req.param("target_user") {
        Ok(target_user) => target_user,
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };

    let units = match req.param("units") {
        Ok(units) => units.parse::<u64>().unwrap(),
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };

    // Create the request with the URL and body data
    let response = isahc::post(
        format!("{}/submit_tx", req.state()),
        mint_bb_tx(target_user, units),
    )
    .expect("Failed to send POST request");

    // Check if the request was successful (status code 2xx)
    if response.status().is_success() {
        info!("POST request successful! Status: {}", response.status());
        TideResult::Ok(Response::new(StatusCode::Ok))
    } else {
        info!("POST request failed! Status: {}", response.status());
        TideResult::Err(anyhow::Error::msg("Error during tx submission").into())
    }
}

async fn submit_transfer(req: Request<String>) -> TideResult {
    let from_user = match req.param("from_user") {
        Ok(from_user) => from_user,
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };
    let to_user = match req.param("to_user") {
        Ok(to_user) => to_user,
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };
    let units = match req.param("units") {
        Ok(units) => units.parse::<u64>().unwrap(),
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };

    // Create the request with the URL and body data
    let response = isahc::post(
        format!("{}/submit_tx", req.state()),
        transfer_bb_tx(from_user, to_user, units, None),
    )
    .expect("Failed to send POST request");

    // Check if the request was successful (status code 2xx)
    if response.status().is_success() {
        info!("POST request successful! Status: {}", response.status());
        TideResult::Ok(Response::new(StatusCode::Ok))
    } else {
        info!("POST request failed! Status: {}", response.status());
        TideResult::Err(anyhow::Error::msg("Error during tx submission").into())
    }
}

async fn test_size(req: Request<String>) -> TideResult {
    let from_user = match req.param("from_user") {
        Ok(from_user) => from_user,
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };
    let to_user = match req.param("to_user") {
        Ok(to_user) => to_user,
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };
    let units = match req.param("units") {
        Ok(units) => units.parse::<u64>().unwrap(),
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };
    let data = match req.param("data_size") {
        Ok(data_size) => {
            vec![0; data_size.parse::<usize>().unwrap()]
        }
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };

    // Create the request with the URL and body data
    let response = isahc::post(
        format!("{}/submit_tx", req.state()),
        transfer_bb_tx(from_user, to_user, units, Some(data)),
    )
    .expect("Failed to send POST request");

    // Check if the request was successful (status code 2xx)
    if response.status().is_success() {
        info!("POST request successful! Status: {}", response.status());
        TideResult::Ok(Response::new(StatusCode::Ok))
    } else {
        info!("POST request failed! Status: {}", response.status());
        TideResult::Err(anyhow::Error::msg("Error during tx submission").into())
    }
}

async fn loop_submit_tx(req: Request<String>) -> TideResult {
    let txs_number = match req.param("number") {
        Ok(txs_number) => txs_number,
        Err(e) => {
            // TODO: handle this
            println!("{e}");
            panic!()
        }
    };

    let txs_number = txs_number.parse::<u64>().unwrap();

    for j in 0..txs_number {
        let body_data = mint_bb_tx("#PIT", 1_000);
        // Create the request with the URL and body data
        let response = isahc::post(format!("{}/submit_tx", req.state()), body_data)
            .expect("Failed to send POST request");

        // Check if the request was successful (status code 2xx)
        if response.status().is_success() {
            info!(
                "POST request number {} successful! Status: {}",
                j,
                response.status()
            );
        } else {
            info!(
                "POST request number {} failed! Status: {}",
                j,
                response.status()
            );
            return TideResult::Err(anyhow::Error::msg("Error during tx submission").into());
        }

        async_std::task::sleep(Duration::from_millis(100)).await;
    }

    TideResult::Ok(Response::new(StatusCode::Ok))
}

async fn publish_mining_sc(req: Request<String>) -> TideResult {
    // Loads-up service module.
    let mut mine_file = std::fs::File::open("trinci_mining.wasm")
        .expect("mining smart contract file not found");
    let mut bin = Vec::new();
    std::io::Read::read_to_end(&mut mine_file, &mut bin).expect("loading mining smart contract");

    let args = ContractRegistrationArgs {
        name: "#TCOIN",
        version: "0.1.0",
        description: "trinci mining coin",
        url: "http://affidaty.io",
        bin: &bin,
    };
    // Create the request with the URL and body data
    let response = isahc::post(format!("{}/submit_tx", req.state()), publish_sc(args))
        .expect("Failed to send POST request");

    // Check if the request was successful (status code 2xx)
    if response.status().is_success() {
        info!("POST request successful! Status: {}", response.status());
        TideResult::Ok(Response::new(StatusCode::Ok))
    } else {
        info!("POST request failed! Status: {}", response.status());
        TideResult::Err(anyhow::Error::msg("Error during tx submission").into())
    }
}

async fn init_mining_sc(req: Request<String>) -> TideResult {
    // Create the request with the URL and body data
    let response = isahc::post(format!("{}/submit_tx", req.state()), craft_init_mining_sc())
        .expect("Failed to send POST request");

    // Check if the request was successful (status code 2xx)
    if response.status().is_success() {
        info!("POST request successful! Status: {}", response.status());
        TideResult::Ok(Response::new(StatusCode::Ok))
    } else {
        info!("POST request failed! Status: {}", response.status());
        TideResult::Err(anyhow::Error::msg("Error during tx submission").into())
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    init_logger(
        &env!("CARGO_PKG_NAME").replace("-", "_"),
        LevelFilter::Error,
        LevelFilter::Info,
    );

    let args = Args::parse();

    let mut app = tide::with_state(args.node_rest_addr);

    app.at("/mint/:target_user/:units").get(submit_mint);
    app.at("/test_size/:from_user/:to_user/:units/:data_size")
        .get(test_size);
    app.at("/transfer/:from_user/:to_user/:units")
        .get(submit_transfer);
    app.at("/loop_submit_txs/:number").get(loop_submit_tx);
    app.at("/publish_mining_sc").get(publish_mining_sc);
    app.at("/init_mining_sc").get(init_mining_sc);

    Ok(app
        .listen(args.local_rest_addr)
        .await
        .map_err(|e| log_error!(e))?)
}
