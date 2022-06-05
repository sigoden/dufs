FROM rust:1.61 as builder
RUN rustup target add x86_64-unknown-linux-musl
RUN apt-get update && apt-get install --no-install-recommends -y musl-tools
WORKDIR /app
COPY . .
RUN cargo build --target x86_64-unknown-linux-musl --release

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/duf /bin/
ENTRYPOINT ["/bin/duf"]