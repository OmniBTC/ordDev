FROM rust:1.69 as builder

RUN apt update \
    && apt install -y libclang-dev

WORKDIR /code
COPY ./ ./
RUN cargo build --release

FROM comingweb3/coming-ubuntu:arm64

# Installing necessary tools
RUN apt update && apt install -y wget tar curl git 

# Install Rust and Cargo with rustup (installing version 1.69 specifically)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --profile minimal --default-toolchain 1.69.0 -y
ENV PATH="/root/.cargo/bin:${PATH}"


# Download and extract Bitcoin binaries to /usr/local/bin
WORKDIR /tmp
RUN wget https://bitcoincore.org/bin/bitcoin-core-25.0/bitcoin-25.0-arm-linux-gnueabihf.tar.gz \
    && tar -xzf bitcoin-25.0-arm-linux-gnueabihf.tar.gz \
    && cp bitcoin-25.0/bin/* /usr/local/bin \
    && rm -rf bitcoin-25.0*

WORKDIR /
COPY --from=builder /code/target/release /usr/local/bin

