use crate::{calc::calculate_prices, token::load_token_info};
use alloy::primitives::Address;
use alloy::{
    providers::Provider,
    rpc::types::{BlockNumberOrTag, Filter},
};
use alloy_sol_types::sol;
use eyre::Result;
use futures_util::stream::StreamExt;
use thiserror::Error;
use tracing::error;
use tracing::info;

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("Failed to fetch token from pool")]
    TokenFetchFailed,

    #[error("Failed to fetch token info from address")]
    TokenInfoFetchFailed,
}

#[derive(Debug, Error)]
pub enum LogError {
    #[error("Failed to subscribe logs")]
    LogSubscriptionFailed,

    #[error("Failed to fetch sqrt price")]
    SqrtPriceFetchFailed,
}

#[derive(Error, Debug)]
pub enum PriceError {
    #[error("Failed to calculate price for pool {0}, tokens {1}/{2}: {3}")]
    CalculationFailed(Address, String, String, String),
}

sol! {
    #[sol(rpc)]
    interface IUniswapV3Pool {
        function token0() external view returns (address);
        function token1() external view returns (address);
    }

    event Swap(
        address indexed sender,
        address indexed recipient,
        int256 amount0,
        int256 amount1,
        uint160 sqrtPriceX96,
        uint128 liquidity,
        int24 tick
    );
}

pub async fn listen_pool(pool_addr: Address, provider: impl Provider) -> Result<()> {
    let pool = IUniswapV3Pool::new(pool_addr, &provider);

    // fetch token0 address from the pool contract
    // returns an Ethereum address for token0
    let token0 = match pool.token0().call().await {
        Ok(addr) => addr,
        Err(e) => {
            error!(
                "Failed to fetch token0 address for pool {}: {}",
                pool_addr, e
            );
            return Err(TokenError::TokenFetchFailed.into());
        }
    };

    // fetch token1 address from the pool contract
    // returns an Ethereum address for token1
    let token1 = match pool.token1().call().await {
        Ok(addr) => addr,
        Err(e) => {
            error!(
                "Failed to fetch token1 address for pool {:?}: {:?}",
                pool_addr, e
            );
            return Err(TokenError::TokenFetchFailed.into());
        }
    };

    //call token contracts with load_token_info function for fetch decimals and symbols
    let (dec0, sym0) = match load_token_info(token0, &provider).await {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to load token info for token {:?}: {}", token0, e);
            return Err(TokenError::TokenInfoFetchFailed.into());
        }
    };
    let (dec1, sym1) = match load_token_info(token1, &provider).await {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to load token info for token {:?}: {}", token1, e);
            return Err(TokenError::TokenInfoFetchFailed.into());
        }
    };

    //filter to listen only for swap events from this pool
    let filter = Filter::new()
        .address(pool_addr)
        .event("Swap(address,address,int256,int256,uint160,uint128,int24)")
        .from_block(BlockNumberOrTag::Latest);

    let sub = match provider.subscribe_logs(&filter).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to subscribe logs with filter {:?}: {}", filter, e);
            return Err(LogError::LogSubscriptionFailed.into());
        }
    };

    let mut stream = sub.into_stream();

    info!("Listening pool: {:?}", pool_addr);

    while let Some(log) = stream.next().await {
        let Swap { sqrtPriceX96, .. } = match log.log_decode() {
            Ok(decoded) => decoded.inner.data,
            Err(e) => {
                tracing::error!("Failed to decode log: {}", e);
                return Err(LogError::SqrtPriceFetchFailed.into());
            }
        };

        //calculate price with sqrtpricex96 and token decimals
        let price = match calculate_prices(
            sqrtPriceX96.to_string(),
            dec0 as u32,
            dec1 as u32,
            &sym0,
            &sym1,
        ) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to calculate price for {}/{}: {}", sym0, sym1, e);
                return Err(PriceError::CalculationFailed(
                    pool_addr,
                    sym0.clone(),
                    sym1.clone(),
                    e.to_string(),
                )
                .into());
            }
        };

        info!("SQRT_PRICE: {:#?} from pool: {:?}", price, pool_addr);
    }

    Ok(())
}
