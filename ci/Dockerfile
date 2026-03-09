# syntax=docker/dockerfile:1
# needs to match the base debian version of distroless container images
# see: https://github.com/GoogleContainerTools/distroless
FROM debian:13
ENV DEBIAN_FRONTEND=noninteractive

# Prepare the unprivileged user environment
RUN groupadd --gid 2000 builder && \
    useradd --uid 2000 --gid 2000 --create-home --shell /bin/bash builder && \
    mkdir -p /cache/cargo && \
    mkdir -p /cache/target && \
    chown -R builder: /cache /home/builder
USER builder

RUN apt-get update && \
    apt-get install -y \
    build-essential \
    curl \
    git \
    git-lfs \
    gpg \
    gtk+-3.0-dev \
    glib-2.0-dev \
    gio-2.0-dev \
    jq \
    ninja-build \
    libclang-dev \
    locales \
    pkg-config \
    ripgrep \
    unzip \
    && \
    rm -rf /var/lib/apt/lists/*

# Set default locale
RUN echo "en_US.UTF-8 UTF-8" > /etc/locale.gen && locale-gen
ENV LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain stable -y
ENV PATH=/home/builder/.cargo/bin:$PATH

# Install tools (and check that they work)
RUN --mount=type=tmpfs,target=/var/tmp \
    cd /var/tmp && \
    curl -sLf https://github.com/casey/just/releases/download/1.40.0/just-1.40.0-x86_64-unknown-linux-musl.tar.gz | tar -zx && \
    curl -sLf https://github.com/EmbarkStudios/cargo-deny/releases/download/0.18.2/cargo-deny-0.18.2-x86_64-unknown-linux-musl.tar.gz | tar --strip-components=1 -zx && \
    curl -sLf https://prx-libs.eu-central-1.linodeobjects.com/Linux/x86_64/cargo-sweep-0.7.0-x86_64-unknown-linux-musl.tar.gz | tar -xz && \
    curl -sLf https://github.com/tamasfe/taplo/releases/download/0.9.3/taplo-full-linux-x86_64.gz -o /usr/local/bin/taplo.gz && gzip -d /usr/local/bin/taplo.gz && chmod +x /usr/local/bin/taplo && \
    curl -sLf https://github.com/crate-ci/typos/releases/download/v1.30.2/typos-v1.30.2-x86_64-unknown-linux-musl.tar.gz | tar -xz && mv typos /usr/local/bin/typos && \
    mv just cargo-deny cargo-sweep /usr/local/bin
RUN just --version
RUN cargo-deny --version
RUN cargo-sweep --version
RUN taplo --version
RUN typos --version

# Install fvm (flutter version manager)
RUN curl -sLf https://github.com/leoafarias/fvm/releases/download/3.2.1/fvm-3.2.1-linux-x64.tar.gz | tar -C /usr/local/lib -xz && \
    ln -sf /usr/local/lib/fvm/fvm /usr/bin/fvm && \
    fvm --version
ENV FVM_CACHE_PATH=/home/builder/.cache/fvm
ENV PUB_CACHE=/home/builder/.cache/pub

USER builder

# Create cache directories accessible by our unprivileged user. These are
# shared across containers (CI jobs) using gitlab-runner configuration.
RUN mkdir -p /home/builder/.cache/fvm

# Mark checkouts from outside as safe
RUN git config --global --add safe.directory '*'

ENV CARGO_HOME=/cache/cargo
ENV CARGO_TARGET_DIR=/cache/target
ENV BUNDLE_CACHE_PATH=/home/builder/.cache/bundle⏎      