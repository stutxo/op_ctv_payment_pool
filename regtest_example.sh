#!/bin/bash

check_container_running() {
    docker inspect --format="{{.State.Running}}" $1 2>/dev/null
}

CONTAINER_NAME="bitcoin-inquisition-regtest"

if [ "$(check_container_running $CONTAINER_NAME)" == "true" ]; then
    echo "Docker container $CONTAINER_NAME is already running."
else
    docker run -d \
      --name $CONTAINER_NAME \
      -p 18443:18443 \
      ghcr.io/stutxo/bitcoin-inq:latest \
      -regtest \
      -server \
      -rpcallowip=0.0.0.0/0 \
      -rpcbind=0.0.0.0 \
      -minrelaytxfee=0 \
      -fallbackfee=0.0001 \
      -rpcuser=ctviscool \
      -rpcpassword=ctviscool \
      -txindex=1

    if [ $? -eq 0 ]; then
        echo "Docker container started successfully."
    else
        echo "Failed to start Docker container."
        exit 1
    fi
fi

export BITCOIN_RPC_USER="ctviscool"
export BITCOIN_RPC_PASS="ctviscool"

cargo run --features "regtest"
