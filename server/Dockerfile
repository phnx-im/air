# SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

ARG RUST_VERSION
FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION}-trixie AS chef
WORKDIR /app

FROM chef AS planner
COPY . .

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

RUN cargo chef cook --release --package airserver --recipe-path recipe.json

COPY . .
ENV SQLX_OFFLINE=true

RUN cargo build --release --package airserver --bin airserver

FROM gcr.io/distroless/cc-debian13 AS runtime

COPY --from=builder /app/target/release/airserver airserver

RUN ["./airserver", "--help"]

COPY server/configuration server/configuration

ENV APP_ENVIRONMENT=production

ENTRYPOINT ["./airserver"]
