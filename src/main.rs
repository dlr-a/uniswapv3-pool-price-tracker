mod calc;
mod pool;
mod token;

use alloy::primitives::Address;
use alloy::providers::{ProviderBuilder, WsConnect};
use eyre::Result;
use pool::listen_pool;
use std::env;
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::{error, info};
use tracing_subscriber;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Failed to connect WS")]
    WSConnectionFailed,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let pools_str = match env::var("POOLS") {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Environment variable POOLS is missing");
            panic!("POOLS environment variable is not set");
        }
    };
    let rpc_url =
        env::var("RPC_URL").unwrap_or_else(|_| "wss://ethereum-rpc.publicnode.com".to_string());

    let ws = WsConnect::new(rpc_url);
    let provider = match ProviderBuilder::new().connect_ws(ws).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to connect WebSocket provider: {}", e);
            return Err(ProviderError::WSConnectionFailed.into());
        }
    };

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    //split pool addresses by commas
    let pool_addresses: Vec<Address> = pools_str
        .split(',')
        .filter_map(|addr| addr.trim().parse().ok())
        .collect();

    info!("Loaded {} pools from .env", pool_addresses.len());

    let mut handles: Vec<JoinHandle<Result<()>>> = Vec::new();

    // spawn a separate async task for each pool
    // each task listens to swaps and updates price info concurrently
    for pool_addr in pool_addresses {
        let provider = provider.clone();
        handles.push(tokio::spawn(async move {
            listen_pool(pool_addr, provider).await
        }));
    }

    for handle in handles {
        match handle.await {
            Ok(task_result) => match task_result {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("Task returned an error: {:?}", e);
                }
            },
            Err(join_err) => {
                tracing::error!("Task panicked: {:?}", join_err);
            }
        }
    }

    Ok(())
}
