FROM rust:1.57.0-alpine

COPY ./target/release/worker /app

CMD cd /app && worker ./config.yml