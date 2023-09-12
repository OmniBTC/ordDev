FROM rust:1.69 as builder

RUN apt update \
    && apt install -y libclang-dev

WORKDIR /code
COPY ./ ./
RUN cargo build --release

FROM ubuntu:latest

# Installing necessary tools
RUN apt update && apt install -y wget tar 

# Download and extract Bitcoin binaries to /usr/local/bin
WORKDIR /tmp
RUN wget https://bitcoincore.org/bin/bitcoin-core-25.0/bitcoin-25.0-arm-linux-gnueabihf.tar.gz \
    && tar -xzf bitcoin-25.0-arm-linux-gnueabihf.tar.gz \
    && cp bitcoin-25.0/bin/* /usr/local/bin \
    && rm -rf bitcoin-25.0*

WORKDIR /code
COPY --from=builder /code/target/release /usr/local/bin

