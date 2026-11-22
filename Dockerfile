FROM rust:1-slim-bookworm AS builder

WORKDIR /build

COPY rust/core ./rust/core
COPY rust/app ./rust/app

WORKDIR /build/rust/app
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime

RUN mkdir -p /data

COPY --from=builder /build/rust/app/target/release/server /usr/local/bin/server

WORKDIR /data

ENV HOST=0.0.0.0 \
    PORT=8080 \
    DATA_DIR=/data \
    NO_CLIENT=1

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/server", "--no-console", "--no-client"]
