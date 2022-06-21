FROM --platform=$BUILDPLATFORM rust:1.61-alpine AS vendor

ENV USER=root

WORKDIR /app
RUN cargo init
COPY Cargo.toml /app/Cargo.toml
COPY Cargo.lock /app/Cargo.lock
RUN mkdir -p /app/.cargo \
  && cargo vendor > /app/.cargo/config
  
FROM rust:1.61-alpine AS builder

RUN apk add ca-certificates openssl-dev openssl musl-dev

ENV USER=root

WORKDIR /app

COPY Cargo.toml /app/Cargo.toml
COPY Cargo.lock /app/Cargo.lock
COPY src /app/src
COPY assets /app/assets
COPY --from=vendor /app/.cargo /app/.cargo
COPY --from=vendor /app/vendor /app/vendor

RUN cargo build --release --offline

FROM gcr.io/distroless/static

COPY --from=builder /app/target/release/dufs /bin/

ENTRYPOINT ["/bin/dufs"]