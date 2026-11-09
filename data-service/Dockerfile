# syntax=docker/dockerfile:1
# Repository-root context. Only the separate network adapter configures identity; guests retain no IO.
FROM rust:1.96-bookworm AS build
WORKDIR /build
COPY LICENSE LICENSE
COPY authority/Cargo.toml authority/
COPY authority/src/ authority/src/
COPY authority/sql/ authority/sql/
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY model/Cargo.toml model/Cargo.lock model/
COPY model/src/ model/src/
COPY data-service/Cargo.toml data-service/Cargo.lock data-service/
COPY data-service/src/ data-service/src/
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/data-service/target cargo build --locked --release --manifest-path data-service/Cargo.toml && cp data-service/target/release/veyndo-app-data-service /build/app-data
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* && useradd -u 10001 -m runtime
WORKDIR /app
COPY --from=build /build/app-data /usr/local/bin/veyndo-app-data-service
USER 10001:10001
ENV APP_DATA_BIND=0.0.0.0:8799
EXPOSE 8799
HEALTHCHECK --interval=30s --timeout=3s CMD curl --fail --silent http://127.0.0.1:8799/health || exit 1
ENTRYPOINT ["veyndo-app-data-service"]
