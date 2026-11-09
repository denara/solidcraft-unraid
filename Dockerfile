# syntax=docker/dockerfile:1
# Build locally; the runtime needs neither a display nor GPU access.
FROM rust:1.99.1-bookworm AS toolchain
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libxkbcommon-dev libwayland-dev libasound2-dev \
    libegl1-mesa-dev libx11-dev libxi-dev libxcursor-dev libxrandr-dev \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown && rustup component add rustfmt clippy
WORKDIR /src
ENV CARGO_TARGET_DIR=/src/target
ENV CARGO_BUILD_JOBS=4
# CI supplies only the verified browser tools through this optional context.
FROM scratch AS web-tools

FROM toolchain AS build
# Pin and verify Trunk's browser tools before compiling either web app.
RUN --mount=type=bind,source=packaging/unraid/install-web-tools.sh,target=/tmp/install-web-tools.sh \
    --mount=type=bind,source=Cargo.lock,target=/tmp/Cargo.lock \
    --mount=type=bind,from=web-tools,target=/tmp/web-tools \
    sh /tmp/install-web-tools.sh install /tmp/Cargo.lock /tmp/web-tools
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
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cd apps/solidcraft-workspace && trunk build --release --locked --public-url /workspace/assets/

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data /opt/solvecraft/web \
    && chown 99:100 /data
COPY --from=build /out/solidcraft-cli /out/solvecraft-cli /out/solvecraft-server /usr/local/bin/
COPY --from=build /src/dist/web/ /opt/solvecraft/web/
COPY --from=build /src/dist/workspace/ /opt/solvecraft/workspace/
COPY LICENSE-MIT LICENSE-APACHE NOTICE THIRD-PARTY-LICENSES.txt /opt/solvecraft/
ENV SOLVECRAFT_LISTEN=0.0.0.0:8080 SOLVECRAFT_DATA_DIR=/data SOLVECRAFT_WEB_DIR=/opt/solvecraft/web
USER 99:100
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/health >/dev/null || exit 1
ENTRYPOINT ["solvecraft-server"]
