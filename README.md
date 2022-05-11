# Concordium toolbox



# Setup

## Consensus
run `stack build` from the `deps/concordium-node/concordium-consensus` directory.

## Genesis
Run the following from the `deps/concordium-node/scripts/genesis` directory.
```bash
USE_DOCKER= PURGE= NUM_BAKERS=5 NUM_EXTRA_ACCOUNTS=20 EXTRA_ACCOUNTS_TEMPLATE=test EXTRA_ACCOUNTS_BALANCE=10000 ./generate-test-genesis.py
```
