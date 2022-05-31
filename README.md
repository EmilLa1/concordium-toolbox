# Concordium 
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

# run the test

## start chain
start the chain via `cargo run` in the `chain/` directory.


## generate transactions
create transactions e.g.
```bash
cargo run -- --node http://127.0.0.1:7000 --sender ../deps/concordium-node/scripts/genesis/genesis_data/bakers/baker-account-0.json --receivers ../deps/concordium-node/scripts/genesis/genesis_data/tests/receivers.json --tps 400
```

where `receivers.json` is the extracted addresses from `deps/concordium-node/scripts/genesis/genesis_data/tests/tests.json`

## analyze blocks
run `cargo run` in the `block-analyzer/` directory. Use `--out foo.csv` to get a csv file. 

## analyze logs
run `cargo run` in the `log-analyzer/` directory.Supply log file with `--in foo.log` Use `--out foo.csv` to get a csv file. 

