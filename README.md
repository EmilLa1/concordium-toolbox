# Concordium toolbox

# Setup

## Consensus
run `stack build` from the `deps/concordium-node/concordium-consensus` directory.

## Genesis
For the poorly connected network create a genesis with 1 baker: 

Run the following from the `deps/concordium-node/scripts/genesis` directory.
```bash
USE_DOCKER= PURGE= NUM_BAKERS=1 NUM_EXTRA_ACCOUNTS=20 EXTRA_ACCOUNTS_TEMPLATE=test EXTRA_ACCOUNTS_BALANCE=10000 ./generate-test-genesis.py
```

Or for an optimal connected network use 

```bash
USE_DOCKER= PURGE= NUM_BAKERS=5 NUM_EXTRA_ACCOUNTS=20 EXTRA_ACCOUNTS_TEMPLATE=test EXTRA_ACCOUNTS_BALANCE=10000 ./generate-test-genesis.py
```

## start chain
start the chain via `cargo run` in the `chain/` directory.

## generate transactions
https://github.com/Concordium/concordium-rust-sdk/blob/main/examples/generator.rs

## analyze blocks
run `cargo run` in the `block-analyzer/` directory. Use `--out foo.csv` to get a csv file. 

## analyze logs
run `cargo run` in the `log-analyzer/` directory.Supply log file with `--in foo.log` Use `--out foo.csv` to get a csv file. 

where `receivers.json` is the extracted addresses from `deps/concordium-node/scripts/genesis/genesis_data/tests/tests.json`

## process monitoring
run `cargo run` in the `process-metrics/` directory. Use `--out foo.csv` to get a csv file.
