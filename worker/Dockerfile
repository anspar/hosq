FROM debian:buster
RUN apt-get update && rm -rf /var/lib/apt/lists/*

COPY ./target/release/worker /app/worker

CMD cd /app && ./worker ./config.yml