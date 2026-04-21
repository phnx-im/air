# SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

FROM ubuntu:24.04

ARG RUST_TOOLCHAIN=1.94.0
ARG FLUTTER_VERSION=3.41.4
ARG FRB_VERSION=2.12.0
ARG JUST_VERSION=1.48.1
ARG SQLX_CLI_VERSION=0.8.6
ARG CARGO_DENY_VERSION=0.19.0
ARG CARGO_MACHETE_VERSION=0.9.1

ARG USERNAME=dev
ARG USER_UID=1000
ARG USER_GID=1000

ENV DEBIAN_FRONTEND=noninteractive
ENV LANG=C.UTF-8

# ── System packages (requires root) ─────────────────────────────────
RUN apt-get update -qq && apt-get install -yqq --no-install-recommends \
    build-essential \
    clang \
    lld \
    pkg-config \
    ninja-build \
    libgtk-3-dev \
    openssl \
    libssl-dev \
    postgresql-client \
    libsqlite3-dev \
    git \
    git-lfs \
    openssh-client \
    curl \
    wget \
    unzip \
    ca-certificates \
    gnupg \
    python3 \
    python3-pip \
    bash \
    && rm -rf /var/lib/apt/lists/*

RUN git lfs install --system

# Node.js + bunyan (JSON log formatter for run-server)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -yqq --no-install-recommends nodejs \
    && npm install -g bunyan \
    && rm -rf /var/lib/apt/lists/*

# Python tools (reuse — SPDX license linter)
RUN pip3 install --break-system-packages --no-cache-dir "reuse~=6.2"

# nfpm (Linux package builder for build-rpm / build-deb)
RUN curl -sfL https://install.goreleaser.com/github.com/goreleaser/nfpm.sh | bash -s -- -b /usr/local/bin

# ── Non-root user ───────────────────────────────────────────────────
RUN if id -u "${USER_UID}" >/dev/null 2>&1; then \
        existing=$(getent passwd "${USER_UID}" | cut -d: -f1) \
        && usermod -l "${USERNAME}" -d "/home/${USERNAME}" -m -s /bin/bash "$existing" \
        && groupmod -n "${USERNAME}" "$(getent group "${USER_GID}" | cut -d: -f1)"; \
    else \
        groupadd --gid "${USER_GID}" "${USERNAME}" \
        && useradd --uid "${USER_UID}" --gid "${USER_GID}" -m -s /bin/bash "${USERNAME}"; \
    fi

# Pre-create directories that will back named volumes so they inherit correct ownership
RUN mkdir -p /workspace/target \
        /home/${USERNAME}/.cargo/registry \
        /home/${USERNAME}/.cargo/git \
    && chown "${USER_UID}:${USER_GID}" /workspace/target \
    && chown -R "${USER_UID}:${USER_GID}" /home/${USERNAME}/.cargo

RUN git config --system --add safe.directory /workspace

COPY .devcontainer/devcontainer-entrypoint.sh /usr/local/bin/devcontainer-entrypoint
RUN chmod 0755 /usr/local/bin/devcontainer-entrypoint

USER ${USERNAME}

# ── User-local tooling (runs as dev) ────────────────────────────────
ENV RUSTUP_HOME="/home/${USERNAME}/.rustup"
ENV CARGO_HOME="/home/${USERNAME}/.cargo"
ENV PATH="${CARGO_HOME}/bin:${PATH}"

# Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain "${RUST_TOOLCHAIN}" \
        --component rustfmt,clippy

# cargo-binstall + cargo tools
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
    | bash \
    && cargo binstall --no-confirm \
        "just@${JUST_VERSION}" \
        "sqlx-cli@${SQLX_CLI_VERSION}" \
        "cargo-deny@${CARGO_DENY_VERSION}" \
        "cargo-machete@${CARGO_MACHETE_VERSION}" \
        cargo-expand \
        "flutter_rust_bridge_codegen@${FRB_VERSION}"

# Flutter
ENV FLUTTER_HOME="/home/${USERNAME}/.flutter-sdk"
ENV PATH="${FLUTTER_HOME}/bin:${FLUTTER_HOME}/bin/cache/dart-sdk/bin:${PATH}"
RUN git clone --depth 1 --branch "${FLUTTER_VERSION}" \
        https://github.com/flutter/flutter.git "${FLUTTER_HOME}" \
    && flutter precache --linux \
    && flutter config --no-analytics \
    && dart --disable-analytics

# ── Runtime environment ─────────────────────────────────────────────
ENV TERM=xterm-256color
ENV CI=true
ENV RUST_BACKTRACE=1
ENV RUSTFLAGS="-D warnings"
ENV SQLX_OFFLINE=true
ENV CARGO_TERM_COLOR=always

WORKDIR /workspace

USER root
ENTRYPOINT ["/usr/local/bin/devcontainer-entrypoint"]
