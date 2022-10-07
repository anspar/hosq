FROM rust:1.64.0-buster

COPY ./src /build/src
COPY ./Cargo.toml /build/Cargo.toml

RUN cd /build && cargo build --release


FROM debian:buster
RUN apt-get update && apt install -y ca-certificates libssl-dev && rm -rf /var/lib/apt/lists/*

RUN mkdir /app

# COPY ./target/release/worker /app/worker
COPY --from=0 /build/target/release/hosq /app
COPY ./Rocket.toml /app

CMD cd /app && ./hosq ./config.yml