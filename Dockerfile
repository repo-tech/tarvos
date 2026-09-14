# Tarvos v1.0.0 hybrid production image.
# Runtime includes Python for the embedded AST exporter and rustc/sysroot for
# generated Rust builds, but never includes the repository source tree or Cargo.

FROM rust:1.80-slim AS builder

WORKDIR /workspace
ENV RUSTFLAGS="-C opt-level=z -C strip=symbols -C target-cpu=native"

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY python ./python

RUN cargo build --locked --release --bin tarvos-server

FROM debian:bookworm-slim AS runtime

ENV PORT=8080 \
    TARVOS_PYTHON=python3 \
    RUSTFLAGS="-C opt-level=3 -C strip=symbols -C panic=abort" \
    CARGO_HOME=/tmp/tarvos-cargo \
    RUSTUP_HOME=/nonexistent \
    PATH="/opt/tarvos/toolchain/bin:${PATH}"

WORKDIR /app

COPY --from=builder /workspace/target/release/tarvos-server /app/tarvos-gateway
COPY --from=builder /usr/local/rustup/toolchains/1.80.1-x86_64-unknown-linux-gnu /opt/tarvos/toolchain

RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /tmp/tarvos-cargo /tmp/tarvos-work \
    && chmod 0555 /app/tarvos-gateway /opt/tarvos/toolchain/bin/rustc \
    && useradd --system --uid 10001 --create-home --home-dir /home/tarvos --shell /usr/sbin/nologin tarvos \
    && chown -R tarvos:tarvos /tmp/tarvos-cargo /tmp/tarvos-work /home/tarvos \
    && rm -rf /opt/tarvos/toolchain/share/doc /opt/tarvos/toolchain/share/man

USER tarvos
EXPOSE 8080

ENTRYPOINT ["/app/tarvos-gateway"]
