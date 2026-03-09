# syntax=docker/dockerfile:1
# This needs to match the base debian version of distroless container images:
# if we build backend services that we run in Kubernetes with the distroless image
# we need a match of glibc to be able to execute the binary.
# see: https://github.com/GoogleContainerTools/distroless
FROM debian:13
ENV DEBIAN_FRONTEND=noninteractive

# Prepare the unprivileged user environment
RUN groupadd --gid 2000 builder && \
    useradd --uid 2000 --gid 2000 --create-home --shell /bin/bash builder && \
    mkdir -p /cache/cargo && \
    mkdir -p /cache/target && \
    chown -R builder: /cache /home/builder

RUN apt-get update && \
    apt-get install -y \
    clang \
    cmake \
    curl \
    git \
    git-lfs \
    jq \
    libgtk-3-dev \
    libstdc++-12-dev \
    ninja-build \
    libclang-dev \
    locales \
    pkg-config \
    unzip \
    && \
    rm -rf /var/lib/apt/lists/*

# Set default locale
RUN echo "en_US.UTF-8 UTF-8" > /etc/locale.gen && locale-gen
ENV LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8

# Everything from this point onwards is unprivileged
USER builder
RUN mkdir -p /home/builder/.local/bin
ENV PATH=/home/builder/.local/bin:$PATH

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain stable -y
ENV PATH=/home/builder/.cargo/bin:$PATH

# Install tools (and check that they work)
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash && \
    cargo binstall -y cargo-deny cargo-sweep just

# Install fvm (flutter version manager)
RUN mkdir /home/builder/.local/lib && \
    cd /home/builder/.local && \
    curl -sLf https://github.com/leoafarias/fvm/releases/download/3.2.1/fvm-3.2.1-linux-x64.tar.gz | tar -C lib -xz fvm && \
    ln -sr lib/fvm/fvm bin/fvm && \
    fvm --version
ENV FVM_CACHE_PATH=/home/builder/.cache/fvm
ENV PUB_CACHE=/home/builder/.cache/pub

# Create cache directories accessible by our unprivileged user. These are
# shared across containers (CI jobs) using gitlab-runner configuration.
RUN mkdir -p ~/.cache/fvm

# Mark checkouts from outside as safe
RUN git config --global --add safe.directory '*'
