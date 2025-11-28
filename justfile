# SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set windows-shell := ["C:\\Program Files\\Git\\bin\\sh.exe","-c"]

export RUST_LOG := "info"
export RUST_BACKTRACE := "1"
export RUSTFLAGS := "-D warnings"


_default:
    just --list

# Reset and migrate databases.
reset-dev:
    cd coreclient && cargo sqlx database reset -y --database-url sqlite:$PWD/client.db
    cd backend && cargo sqlx database reset -y

# Run fast and simple Rust lints.
@check-rust:
    just _check-status "cargo machete"
    just _check-status "reuse lint -l"
    just _check-status "cargo metadata --format-version 1 --locked > /dev/null"
    just _check-status "cargo fmt -- --check"
    just _check-status "cargo deny check"
    just _check-unstaged-changes "git diff"

    # It's fine if this fails on CI, but this is useful for devs
    just _check-unstaged-changes "just regenerate-sqlx || echo 'Database not configured?'"

    echo "{{BOLD}}check-rust done{{NORMAL}}"

# Run fast and simple Flutter lints.
@check-flutter:
    just _check-status "git lfs --version"
    just _check-unstaged-changes "git diff"
    just _check-unstaged-changes "cd app && fvm flutter pub get"
    just _check-unstaged-changes "cd app/rust_builder/cargokit/build_tool && fvm flutter pub get"
    just _check-unstaged-changes "cd app && fvm dart format ."
    just _check-status "cd app && fvm flutter analyze --no-pub"
    just _check-unstaged-changes "just regenerate-l10n"
    echo "{{BOLD}}check-flutter done{{NORMAL}}"

# Run Flutter-Rust bridge lint.
@check-frb:
    just _check-unstaged-changes "just regenerate-frb"

# Run all fast and simple lints.
@check: check-rust check-flutter check-frb

# This task will run the command and hide stdout and stderr. If the command fails, it prints the logs and the task fails.
_check-status command:
    #!/usr/bin/env -S bash -eu
    echo "{{BOLD}}Running {{command}}{{NORMAL}}"
    if ! log=$({{command}} 2>&1); then
        echo "{{RED}}$log{{NORMAL}}" >&2
        just _log-error "{{command}}"
    fi

# This task will run the command and hide stdout. If git diff then reports unstaged changes, the task will fail.
_check-unstaged-changes command:
    #!/usr/bin/env -S bash -eu
    echo "{{BOLD}}Running {{command}}{{NORMAL}}"
    {{command}} >/dev/null
    if ! git diff --quiet; then
        echo -e "{{RED}}Found unstaged changes.{{NORMAL}}"
        just _log-error "{{command}}"
    fi

# This task will print the error and call exit 1. If this is running in GitHub CI, it will add the error to the GitHub summary as an annotation.
_log-error msg:
    #!/usr/bin/env -S bash -eu
    if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
        echo -e "::error::{{msg}}"
    else
        msg="\x1b[1;31mERROR: {{msg}}\x1b[0m"
        echo -e "$msg"
    fi
    exit 1


# Regenerate frb and l10n.
regenerate: regenerate-frb regenerate-l10n regenerate-sqlx

# Regenerate Flutter-Rust bridge files.
[working-directory: 'app']
regenerate-frb:
    rm -f ../applogic/src/frb_*.rs
    touch ../applogic/src/frb_generated.rs
    rm -Rf lib/core/api lib/core/frb_*.dart lib/core/lib.dart

    CARGO_TARGET_DIR="{{justfile_directory()}}/target/frb_codegen" \
        flutter_rust_bridge_codegen generate

    cd .. && cargo fmt

# Regenerate localization files.
regenerate-l10n:
    cd app && cargo xtask prune-unused-l10n # pass --apply and optionally --safe to prevent data loss
    cd app && fvm flutter gen-l10n

regenerate-sqlx:
    cd coreclient && cargo sqlx prepare --database-url sqlite:$PWD/client.db
    cd backend && cargo sqlx prepare

# Run cargo build, clippy and test.
@test-rust: start-docker-compose
    just _check-status "cargo clippy --locked --all-targets"
    just _check-status "cargo test --locked -q"
    echo "{{BOLD}}test-rust done{{NORMAL}}"

# Run flutter test.
test-flutter:
    cd app && fvm flutter test
    @echo "{{BOLD}}test-flutter done{{NORMAL}}"

# Run all lints and tests.
ci: check test

# Run all tests.
test: test-rust test-flutter

docker-is-podman := if `command -v podman || true` =~ ".*podman$" { "true" } else { "false" }
# Run docker compose services in the background.
@start-docker-compose: _generate-db-certs
    if {{docker-is-podman}} == "true"; then \
        podman rm air_minio-setup_1 -i 2>&1 /dev/null; \
        podman-compose --podman-run-args=--replace up -d; \
        podman-compose ps; \
        podman logs air_postgres_1; \
    else \
        docker compose up --wait --wait-timeout=300; \
        docker compose ps; \
    fi

# Generate postgres TLS certificates.
_generate-db-certs:
    cd backend && TEST_CERT_DIR_NAME=test_certs scripts/generate_test_certs.sh

# Use the current test results as new reference images.
update-flutter-goldens:
    fvm flutter test --update-goldens

# Start the client in debug mode.
run-client *args='':
    cd app && fvm flutter run {{args}}

# Start the client from the last debug build.
run-client-cached device="macos":
    #!/usr/bin/env -S bash -eu
    app/build/{{device}}/Build/Products/Debug/Air.app/Contents/*/Air

# Start the server.
run-server:
    cargo run --bin airserver | bunyan

# Increment minor version numbers and update changelog.
bump-version:
    cargo xtask bump-version
