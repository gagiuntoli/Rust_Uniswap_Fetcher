# rust-uniswap-task

## Introduction

[Uniswap](https://docs.uniswap.org/protocol/introduction) is an open-source protocol that allows users to exchange crypto-currencies. It is implemented as a set of smart contracts, these are pieces of code and associated state which are stored on the Ethereum blockchain. Users can perform an exchange by calling the [`swap` function](https://github.com/Uniswap/v3-core/blob/412d9b236a1e75a98568d49b1aeb21e3a1430544/contracts/UniswapV3Pool.sol#L596) by submitting a signed Ethereum transaction to the Uniswap pool smart contract associated with the pair of crypto-currencies they wish to swap. Note you do not need to understand Uniswap's code or inner workings.

When a swap is performed it emits an "event". You can see the latest events output from the DAI/USDC Uniswap pool smart contract [here](https://etherscan.io/address/0x5777d92f208679db4b9778590fa3cab3ac9e2168#events). These events contain information about the swap, such as what account/address provided the input currency (`sender`), the account/address that received the output (`receiver`), and the amounts of each currency that were provided. Note the direction of the swap (USDC -> DAI or DAI -> USDC) is implied by one of `amount0`/DAI or `amount1`/USDC values being negative. The negative indicates the amount output to the `receiver` address. For example 1000 `amount0`/DAI and -50 `amount1`/USDC indicates a swap direction of DAI -> USDC.

A contract may output multiple different types of event such as "Swap", "Mint", "Burn" etc. These are defined in the contract's ABI. The type of the event is indicated by it's signature. You can see this being used in the provided skeleton code.

## Tasks

These tasks should take you around 2-4 hours. If you cannot finish them approximately inside this time frame please submit the incomplete work instead of continuing. In this case during the follow up interview we will discuss how it could have been done, so please be prepared to explain how you would have approached the elements you weren't able to complete.

This repository contains a pre-initialized rust project you should use as the basis of your solutions by creating your own **private** clone of this repository. Some code has been provided in [main.rs](./src/main.rs) to help you start, but you should modify this initial code in anyway you see fit.

Please use the commands `cargo fmt`, `cargo build`, `cargo run`, and `cargo test` to respectively format, build, run, and test your solutions. You should ensure these commands succeed before submitting a complete solution. Note we have included the files [.rustfmt.toml](./.rustfmt.toml) and [rust-toolchain.toml](./rust-toolchain.toml) which control the formatter configuration and the rust version your code should compile against respectively.

To interact with the Ethereum blockchain you need to connect to an Ethereum node/endpoint so you can make RPC requests, for example to request the events emitted from a smart contract at a particular block. [`Infura`](https://infura.io/) is a free service that provides endpoints you can connect to via a URL. This [guide](https://blog.infura.io/post/getting-started-with-infura-28e41844cc89) will help you set this up. To make RPC requests related to the DAI/USDC Uniswap pool smart contract you will need its ABI which we have provided [here](./src/contracts/uniswap_pool_abi.json), and the smart contract's address which is `0x5777d92f208679db4b9778590fa3cab3ac9e2168`.

You may use third-party rust crates, for example you may find it easier to use existing rust crates such as [rust-web3](https://github.com/tomusdrw/rust-web3) for interacting with your Ethereum endpoint, as we have in the provided skeleton.

### I.

Build a rust application that starting from the latest Ethereum block for each block prints the details of all swaps that occurred in that block on the DAI/USDC Uniswap pool on the Ethereum [mainnet](https://ethereum.org/en/developers/docs/networks/). The details should include:
- The amounts as *decimal numbers*. For example if a swap was from 1.5 DAI to 17.8 USDC it should print those amounts as "1.5" and "17.8" respectively. Note in the events these are stored as fixed point numbers in a *signed 256 bit integer*, and the amounts of USDC and DAI are stored with 10^-6 and 10^-18 precision respectively. So 1 USDC is stored as 1000000.
- The "direction" of the swap.
- The sender address.
- The receiver address.

### II.

The Ethereum blockchain is effectively a sequence/chain of changes/blocks to a database that all the Ethereum nodes agree have occurred. Sometimes the tail of the agreed upon chain may change via a "reorganisation". Typically this will only change the last 1-2 blocks. The number of blocks that are changed is referred to as the depth of the reorganisation. Because of this behaviour if you read information output from an endpoint, you may see data/changes/events that are later removed from the agreed upon chain by a reorganisation. Therefore those things are treated as if they did not happen. To help protect against this you can wait a number of blocks before treating information you receive from the endpoint as true.

Add reorganisation protection so that events from block N will not be printed until block N + 5 has been output from the node. If a reorganisation with a depth greater than 5 blocks occurs the application should exit. This protection should be unit tested to demonstrate it can handle all cases correctly.

## Submission

Once the tasks are complete, please inform us via email and share your git repo via zip file. Please ensure that the git history is intact when submitting your completed tasks.
