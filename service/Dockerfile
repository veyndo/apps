# syntax=docker/dockerfile:1
# Repository-root context. Only the separate network adapter configures identity; guests retain no IO.
FROM rust:1.96-bookworm AS build
WORKDIR /build
COPY LICENSE LICENSE
COPY model/Cargo.toml model/Cargo.lock model/
COPY model/src/ model/src/
COPY runtime/Cargo.toml runtime/Cargo.lock runtime/
COPY runtime/src/ runtime/src/
COPY runtime/wit/ runtime/wit/
COPY service/Cargo.toml service/Cargo.lock service/
COPY service/src/ service/src/
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/service/target cargo build --locked --release --manifest-path service/Cargo.toml && cp service/target/release/veyndo-app-runtime-service /build/app-runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* && useradd -u 10001 -m runtime
WORKDIR /app
COPY --from=build /build/app-runtime /usr/local/bin/veyndo-app-runtime-service
USER 10001:10001
ENV APP_RUNTIME_BIND=0.0.0.0:8798
EXPOSE 8798
HEALTHCHECK --interval=30s --timeout=3s CMD curl --fail --silent http://127.0.0.1:8798/health || exit 1
ENTRYPOINT ["veyndo-app-runtime-service"]
