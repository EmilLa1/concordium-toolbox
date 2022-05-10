#!/bin/bash
set -e
if [ -z $1 ];
then
  echo "Missing node id " >&2
  exit 1
fi
BAKER=$1
shift 1

cd ../deps/concordium-node/concordium-node/
mkdir -p baker-$BAKER

GENESIS_ROOT=../scripts/genesis/genesis_data

cp $GENESIS_ROOT/genesis.dat baker-$BAKER

CONNECTIONS=""
# connect to all the bakers on addresses 8000..800$BAKER
for i in $(seq 0 4)
do
  if [ $i -ne $BAKER ]
  then 
    CONNECTIONS=$CONNECTIONS" --connect-to=127.0.0.1:800$i"
  fi
done

export CONCORDIUM_NODE_CONNECTION_NO_BOOTSTRAP_DNS=1
export CONCORDIUM_NODE_CONNECTION_NO_DNSSEC=1
export CONCORDIUM_NODE_ID=$(printf '%x' $BAKER)
export CONCORDIUM_NODE_CONFIG_DIR=baker-$BAKER
export CONCORDIUM_NODE_DATA_DIR=baker-$BAKER
export CONCORDIUM_NODE_BAKER_CREDENTIALS_FILE=$GENESIS_ROOT/bakers/baker-$BAKER-credentials.json
export CONCORDIUM_NODE_RPC_SERVER_PORT=700$BAKER
export CONCORDIUM_NODE_LISTEN_PORT=800$BAKER
export CONCORDIUM_NODE_LISTEN_ADDRESS=0.0.0.0
export CONCORDIUM_NODE_BAKER_HASKELL_RTS_FLAGS=-N2
export CONCORDIUM_NODE_CONNECTION_HOUSEKEEPING_INTERVAL=5
export CONCORDIUM_NODE_CONNECTION_DESIRED_NODES=4

COMMAND="cargo run --release --quiet --"
RUST_BACKTRACE=full $COMMAND\
               $CONNECTIONS "$@"
