# syntax=docker/dockerfile:1
# Build locally; the runtime needs neither a display nor GPU access.
FROM rust:1.95-bookworm AS toolchain
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libxkbcommon-dev libwayland-dev libasound2-dev \
    libegl1-mesa-dev libx11-dev libxi-dev libxcursor-dev libxrandr-dev \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown && rustup component add rustfmt clippy
# Same Trunk version as upstream's browser build. Build tooling is not shipped.
ARG TRUNK_VERSION=0.21.14
RUN curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin
WORKDIR /src
ENV CARGO_TARGET_DIR=/src/target
ENV CARGO_BUILD_JOBS=4
FROM toolchain AS build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p solidcraft-cli -p solvecraft-server \
    && mkdir -p /out \
    && cp target/release/solidcraft-cli target/release/solvecraft-cli target/release/solvecraft-server /out/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cd apps/solidcraft-web && trunk build --release --locked --public-url ./

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data /opt/solvecraft/web \
    && chown 99:100 /data
COPY --from=build /out/solidcraft-cli /out/solvecraft-cli /out/solvecraft-server /usr/local/bin/
COPY --from=build /src/dist/web/ /opt/solvecraft/web/
COPY LICENSE-MIT LICENSE-APACHE NOTICE THIRD-PARTY-LICENSES.txt /opt/solvecraft/
ENV SOLVECRAFT_LISTEN=0.0.0.0:8080 SOLVECRAFT_DATA_DIR=/data SOLVECRAFT_WEB_DIR=/opt/solvecraft/web
USER 99:100
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/health >/dev/null || exit 1
ENTRYPOINT ["solvecraft-server"]
