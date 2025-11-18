### Uniswap V3 Pool Tracker

A Rust-based real-time tracker built with Alloy that listens to Uniswap V3 pool swap events over WebSocket and derives token prices from each event's `sqrtPriceX96` value.

**This project is for practice and learning purposes only; use it carefully and do not rely on it in production or critical environments.**

## Features

- Connects to Ethereum mainnet through WebSocket (Alchemy or Public Node)

- Dynamically loads multiple pool addresses from a .env file

- Listens for Swap events in each pool concurrently

- Fetches pool token addresses, symbols, and decimals

- Calculates price ratios from sqrtPriceX96

- Logs real-time token-to-token prices

## Requirements

- Rust

- Cargo

## Installation

Clone the repository:

`git clone https://github.com/dlr-a/uniswapv3-pool-price-tracker.git`

`cd uniswapv3-pool-price-tracker`

`cargo build`

## Environment Configuration

Create a .env file in the project root and add your pool addresses. Each pool address should be separated by commas. Set your RPC URL with your own provider (e.g., Alchemy) as shown below, or the tracker will automatically connect to the public node at wss://ethereum-rpc.publicnode.com.

Example .env:

`POOLS=poolAddress1,poolAddress2`

`RPC_URL=YOUR_RPC_URL`

## Run the tracker using Cargo

Start the project using Cargo:

`cargo run`
