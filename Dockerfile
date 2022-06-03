FROM rust:1.61 as builder
RUN rustup target add x86_64-unknown-linux-musl
RUN apt update && apt install -y musl-tools musl-dev
WORKDIR /app
COPY . .
RUN cargo build --target x86_64-unknown-linux-musl --release

FROM alpine
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/duf ./
USER 1000:1000
ENTRYPOINT ["/app/duf"]