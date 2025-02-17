FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    build-essential \
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
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r bitcoin \
    && mkdir /home/bitcoin \
    && chown bitcoin:bitcoin /home/bitcoin

USER bitcoin
WORKDIR /home/bitcoin

RUN git clone https://github.com/bitcoin-inquisition/bitcoin.git \
    && cd bitcoin \
    && git checkout v28.1-inq

WORKDIR /home/bitcoin/bitcoin
RUN ./autogen.sh \
    && ./configure --without-gui \
    && make -j$(nproc)

RUN mkdir /home/bitcoin/.bitcoin

EXPOSE 18443 38332

VOLUME ["/home/bitcoin/.bitcoin"]

ENTRYPOINT [ "./src/bitcoind" ]
CMD [ "-printtoconsole" ]
