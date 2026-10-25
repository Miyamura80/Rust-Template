# syntax=docker/dockerfile:1

# ---- Builder ----------------------------------------------------------------
FROM rust:1.99.1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8 AS builder
WORKDIR /app

# Cache dependencies first for faster rebuilds.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# `appctl new` embeds this template via include_str!; the (ungated) scaffold
# module needs it present at compile time, so it must be in the build context.
COPY templates ./templates
RUN cargo build --locked --release -p appctl

# ---- Runtime ----------------------------------------------------------------
FROM debian:bookworm-20261020-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS runtime

# TLS roots for outbound HTTPS (e.g. the http_request command).
# ca-certificates: listed explicitly so each build pulls the post-eMudhra trust store
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/appctl /usr/local/bin/appctl
# The config crate resolves its YAML relative to a compile-time path that does
# not exist in this slim image, so ship the file and point APP_CONFIG_PATH at it.
COPY --from=builder /app/crates/config/global_config.yaml /etc/appctl/global_config.yaml
ENV APP_CONFIG_PATH=/etc/appctl/global_config.yaml

# Override individual values with APP__* env vars. Bind all interfaces in-container.
ENV APP__SERVER__HOST=0.0.0.0
ENV APP__SERVER__PORT=8080
EXPOSE 8080

# Drop root: run the server as a dedicated unprivileged user.
RUN useradd --system --no-create-home --uid 10001 appctl
USER appctl

ENTRYPOINT ["appctl", "serve"]
