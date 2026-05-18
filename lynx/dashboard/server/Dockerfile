FROM rust:1.87-alpine AS builder
WORKDIR /app

RUN apk add --no-cache musl-dev pkgconfig openssl-dev

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs

COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM alpine:3.21 AS runner
RUN apk add --no-cache ca-certificates curl

RUN addgroup -S lynx && adduser -S lynx -G lynx

COPY --from=builder /app/target/release/lynx-dashboard-server /usr/local/bin/server

USER lynx
EXPOSE 8080

HEALTHCHECK --interval=10s --timeout=5s --retries=5 \
  CMD curl -f http://localhost:8080/health || exit 1

CMD ["/usr/local/bin/server"]
