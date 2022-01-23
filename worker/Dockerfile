FROM alpine:3.14

COPY ./target/release/worker /app/worker

CMD cd /app && ./worker ./config.yml