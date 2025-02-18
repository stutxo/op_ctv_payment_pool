FROM ubuntu:22.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    build-essential \
    libssl-dev \
    libtool \
    autotools-dev \
    automake \
    pkg-config \
    bsdmainutils \
    python3 \
    libevent-dev \
    libsqlite3-dev \
    libminiupnpc-dev \
    libzmq3-dev \
    libboost-dev \
    libprotobuf-dev \
    protobuf-compiler \
    libqrencode-dev \
    git \
    curl \
    libclang-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r bitcoin \
    && mkdir /home/bitcoin \
    && chown bitcoin:bitcoin /home/bitcoin

USER bitcoin
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/home/bitcoin/.cargo/bin:${PATH}"
WORKDIR /home/bitcoin

RUN git clone https://github.com/bitcoin-inquisition/bitcoin.git \
    && cd bitcoin \
    && git checkout v28.1-inq \
    && ./autogen.sh \
    && ./configure --without-gui \
    && make -j$(nproc)

RUN git clone https://github.com/RCasatta/fbbe /home/bitcoin/fbbe \
    && cd /home/bitcoin/fbbe \
    && cargo build --release

FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    libssl-dev \
    libevent-dev \
    libsqlite3-dev \
    libminiupnpc-dev \
    libzmq3-dev \
    libboost-dev \
    libprotobuf-dev \
    libqrencode-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r bitcoin \
    && mkdir /home/bitcoin \
    && chown bitcoin:bitcoin /home/bitcoin

USER root
COPY docker-entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
RUN chown bitcoin:bitcoin /usr/local/bin/docker-entrypoint.sh
USER bitcoin

WORKDIR /home/bitcoin

COPY --from=builder /home/bitcoin/bitcoin/src/bitcoind /usr/local/bin/
COPY --from=builder /home/bitcoin/bitcoin/src/bitcoin-cli /usr/local/bin/
COPY --from=builder /home/bitcoin/fbbe/target/release/fbbe /usr/local/bin/

RUN chmod +x /usr/local/bin/docker-entrypoint.sh # Ensure the entrypoint script is executable
RUN mkdir /home/bitcoin/.bitcoin

EXPOSE 18443 38332 3003

VOLUME ["/home/bitcoin/.bitcoin"]

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["-regtest", "-server", "-rpcallowip=0.0.0.0/0", "-rpcbind=0.0.0.0", "-minrelaytxfee=0", "-fallbackfee=0.0001", "-rpcuser=ctviscool", "-rpcpassword=ctviscool", "-txindex=1", "-rest=1"]
