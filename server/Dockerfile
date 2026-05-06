# SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

FROM lukemathwalker/cargo-chef:latest-rust-latest AS chef
WORKDIR /app

FROM chef AS planner
COPY . .

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
ENV SQLX_OFFLINE=true

RUN cargo build --profile=ci-release --bin airserver

FROM debian:bookworm-slim AS runtime

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    # Clean up
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/airserver airserver

COPY server/configuration server/configuration

ENV APP_ENVIRONMENT=production

ENTRYPOINT ["./airserver"]
