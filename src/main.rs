use alloy::{
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::{BlockNumberOrTag, Filter},
};
use alloy_sol_types::sol;
use eyre::Result;
use futures_util::stream::StreamExt;
use num_bigint::BigInt;
use num_rational::Ratio;
use num_traits::One;
use num_traits::Zero;
use std::env;
use uniswap_sdk_core::prelude::*;

fn format_price_readable(value: &BigInt, scale: u32, symbol: &str) -> String {
    let scale_factor = BigInt::from(10u64.pow(scale));
    let int_part = value / &scale_factor;
    let frac_part = value % &scale_factor;

    let mut int_str = int_part.to_string();

    let mut with_commas = String::new();
    let chars: Vec<char> = int_str.chars().collect();
    for (i, c) in chars.iter().rev().enumerate() {
        if i != 0 && i % 3 == 0 {
            with_commas.push(',');
        }
        with_commas.push(*c);
    }
    int_str = with_commas.chars().rev().collect();

    let mut frac_str = frac_part.to_string();
    let missing_zeros = scale as usize - frac_str.len();
    if missing_zeros > 0 {
        frac_str = "0".repeat(missing_zeros) + &frac_str;
    }

    let frac_trimmed = &frac_str;

    if int_part.is_zero() {
        return format!("0.{} {}", frac_trimmed.trim_end_matches('0'), symbol);
    }

    if frac_trimmed.is_empty() {
        format!("{} {}", int_str, symbol)
    } else {
        format!(
            "{}.{} {}",
            int_str,
            frac_trimmed.trim_end_matches('0'),
            symbol
        )
    }
}

fn calculate_prices(
    sqrt_price_x96_str: String,
    decimal_token0: u32,
    decimal_token1: u32,
    token0_symbol: &String,
    token1_symbol: &String,
) -> (BigInt, BigInt) {
    let sqrt_price_x96 = BigInt::parse_bytes(sqrt_price_x96_str.as_bytes(), 10).unwrap();
    let two_pow_96: BigInt = BigInt::one() << 96;

    // (sqrtPriceX96 / 2^96)^2
    let price_ratio = Ratio::new(sqrt_price_x96.clone(), two_pow_96.clone()).pow(2);

    // decimal factor = 10^(dec1 - dec0)
    let decimal_factor = Ratio::new(
        BigInt::from(10u64.pow(decimal_token1)),
        BigInt::from(10u64.pow(decimal_token0)),
    );

    let buy_one_token0_ratio: Ratio<BigInt> = price_ratio / decimal_factor;
    let buy_one_token1_ratio: Ratio<BigInt> = Ratio::one() / &buy_one_token0_ratio;

    // scale = 10^18
    let scale = BigInt::from(10u64.pow(18));

    let buy_one_token0 = (buy_one_token0_ratio.clone() * &scale).to_integer();
    let buy_one_token1 = (buy_one_token1_ratio.clone() * &scale).to_integer();

    println!(
        "Price token0→token1: {}",
        format_price_readable(&buy_one_token0, 18, token1_symbol)
    );
    println!(
        "Price token1→token0: {}",
        format_price_readable(&buy_one_token1, 18, token0_symbol)
    );

    (buy_one_token0, buy_one_token1)
}

sol! {
    #[sol(rpc)]
    interface IUniswapV3Pool {
        function token0() external view returns (address);
        function token1() external view returns (address);
    }

    #[sol(rpc)]
    interface IERC20 {
        function decimals() external view returns (uint8);
        function symbol() external view returns (string);
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

#[tokio::main]
async fn main() -> Result<()> {
    let rpc_url = "wss://ethereum-rpc.publicnode.com";
    dotenvy::dotenv().ok();
    let pools_str = env::var("POOLS")?;
    let ws = WsConnect::new(rpc_url);
    let provider = ProviderBuilder::new().connect_ws(ws).await?;

    let pool_addresses: Vec<Address> = pools_str
        .split(',')
        .filter_map(|addr| addr.trim().parse().ok())
        .collect();

    println!("Loaded {} pools from .env", pool_addresses.len());

    for pool_addr in pool_addresses {
        let provider = provider.clone();

        tokio::spawn(async move {
            let pool = IUniswapV3Pool::new(pool_addr, &provider);

            let token0 = pool.token0().call().await.unwrap();
            let token1 = pool.token1().call().await.unwrap();

            let token0_contract = IERC20::new(token0, &provider);
            let token1_contract = IERC20::new(token1, &provider);

            let dec0 = token0_contract.decimals().call().await.unwrap();
            let dec1 = token1_contract.decimals().call().await.unwrap();

            let sym0 = token0_contract.symbol().call().await.unwrap();
            let sym1 = token1_contract.symbol().call().await.unwrap();

            let filter = Filter::new()
                .address(pool_addr)
                .event("Swap(address,address,int256,int256,uint160,uint128,int24)")
                .from_block(BlockNumberOrTag::Latest);

            let sub = provider.subscribe_logs(&filter).await.unwrap();
            let mut stream = sub.into_stream();

            println!("🌀 Listening pool: {:?}", pool_addr);

            while let Some(log) = stream.next().await {
                let Swap {
                    sender: _,
                    recipient: _,
                    amount0: _,
                    amount1: _,
                    sqrtPriceX96,
                    liquidity: _,
                    tick: _,
                } = log.log_decode().unwrap().inner.data;

                let price = calculate_prices(
                    sqrtPriceX96.to_string(),
                    dec0 as u32,
                    dec1 as u32,
                    &sym0,
                    &sym1,
                );

                println!("SQRT_PRICE:, {:#?} from pool: {:?}", price, pool_addr);
            }
        });
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}
