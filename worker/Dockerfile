FROM debian:buster
RUN apt-get update && apt install -y ca-certificates libssl-dev && rm -rf /var/lib/apt/lists/*

COPY ./target/release/worker /app/worker

CMD cd /app && ./worker ./config.yml