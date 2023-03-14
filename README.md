# Description

This programs prints swap events information from the Uniswap USDC/DAI protocol.
In case a block reorganization of the last 5 blocks takes place the program
stops automatically.

# Instructions

1. Creates an `.env` file in the main folder with a valid Infura web sockets
endpoint, e.g:

```bash
INFURA_WSS_ENDPOINT=wss://mainnet.infura.io/ws/v3/<INFURA_KEY>
```

2. Build the project

```bash
cargo build
```

3. Check the unit tests pass:

```bash
cargo test
```

4. Run the main program:

```bash
cargo run
```

You will see in the console the current block numbers and hashes, as well as for
the current block number - 5. The programs prints swap log events that happened
in not reorganized blocks (current block number - 5).