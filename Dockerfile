FROM rust:slim-bookworm AS builder

WORKDIR /build

RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

RUN mkdir src && \
    echo "fn main() {println!(\"dummy\");}" > src/main.rs

RUN cargo build --release

RUN rm -f target/release/deps/bluesky_random_labeler*
RUN rm src/main.rs

COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && \
    apt-get install -y libssl3 ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/bluesky-random-labeler /usr/local/bin/app

RUN mkdir -p /data

ENV PORT=3000
ENV DATABASE_URL=sqlite:///data/labels.db
ENV RUST_LOG=info

EXPOSE 3000

CMD ["/usr/local/bin/app"]
