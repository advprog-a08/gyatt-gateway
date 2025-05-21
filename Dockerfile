FROM rust:1.86-alpine AS builder

RUN apk add --no-cache musl-dev gcc make pkgconfig openssl-dev openssl-libs-static

WORKDIR /app

COPY . .

RUN rustup target add x86_64-unknown-linux-musl && \
    cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:3.21

RUN apk add --no-cache libgcc

WORKDIR /app

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/proxy .

EXPOSE 5000

ENTRYPOINT [ "./proxy" ]
