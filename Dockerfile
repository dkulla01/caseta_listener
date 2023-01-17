FROM rust:latest AS rust
RUN rustup target add aarch64-unknown-linux-gnu
WORKDIR /caseta_listener
COPY .cargo
