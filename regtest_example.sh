#!/bin/bash

echo "Pulling the latest Docker image..."
docker pull ghcr.io/stutxo/grugpool-regtest:latest

if [ $? -eq 0 ]; then
    echo "Successfully pulled the Docker image."
else
    echo "Failed to pull Docker image."
    exit 1
fi

CONTAINER_NAME="bitcoin-inquisition-regtest"

# Check if the container exists and capture its running state
container_status=$(docker inspect --format="{{.State.Running}}" $CONTAINER_NAME 2>/dev/null)

if [ $? -eq 0 ]; then
    # If docker inspect command was successful, check if the container is running
    if [ "$container_status" == "true" ]; then
        echo "Docker container $CONTAINER_NAME is already running. No action needed."
    else
        echo "Container $CONTAINER_NAME exists but is not running. Starting it now..."
        docker start $CONTAINER_NAME
        if [ $? -eq 0 ]; then
            echo "Docker container started successfully."
        else
            echo "Failed to start Docker container."
            exit 1
        fi
    fi
else
    echo "Container does not exist. Creating and starting it now..."
    docker run -d \
      --name $CONTAINER_NAME \
      -p 18443:18443 \
      -p 3003:3003 \
      ghcr.io/stutxo/grugpool-regtest:latest

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

echo
echo -e "fbbe blockexplorer is running at http://localhost:3003"
