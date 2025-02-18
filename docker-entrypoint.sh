#!/bin/bash

/usr/local/bin/bitcoind -daemon "$@"

# Wait for bitcoind to fully start
sleep 5

# Run fbbe from the correct location
/usr/local/bin/fbbe --network regtest --local-addr 0.0.0.0:3003
