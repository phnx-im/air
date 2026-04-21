# SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set windows-shell := ["C:\\Program Files\\Git\\bin\\sh.exe","-c"]

export RUST_BACKTRACE := "1"
export RUSTFLAGS := "-D warnings"

build_number := `git rev-list --count HEAD`
ci := env_var_or_default("CI", "false")

_default:
    just --list

POSTGRES_HOST := env_var_or_default("POSTGRES_HOST", "localhost")
SERVER_DATABASE_URL := "postgres://postgres:password@" + POSTGRES_HOST + ":5432/air_db"
CLIENT_DATABASE_URL := if os() == "windows" {
    "sqlite:///" + replace(justfile_directory(), "\\", "/") + "/coreclient/client.db"
} else {
    "sqlite://" + justfile_directory() + "/coreclient/client.db"
}

[working-directory('app')]
@dart *args:
    {{ if ci == "true" { "dart" } else { "fvm dart" } }} {{ args }}

[working-directory('app')]
@flutter *args:
    {{ if ci == "true" { "flutter" } else { "fvm flutter" } }} {{ args }}

# Reset and migrate databases.
reset-dev:
    cd coreclient && cargo sqlx database reset -y --database-url {{CLIENT_DATABASE_URL}}
    cd backend && cargo sqlx database reset -y --database-url {{SERVER_DATABASE_URL}}

# Migrate databases.
migrate-dev:
    cd coreclient && cargo sqlx migrate run --database-url {{CLIENT_DATABASE_URL}}
    cd backend && cargo sqlx migrate run --database-url {{SERVER_DATABASE_URL}}

# Run fast and simple Rust lints.
@check-rust:
    just _check-status "cargo machete"
    just _check-status "reuse lint -l"
    just _check-status "cargo metadata --format-version=1 --locked > /dev/null"
    just _check-status "cargo fmt -- --check"
    just _check-status "cargo deny fetch && cargo deny check"
    just _check-unstaged-changes "git --no-pager diff"
    just _check-unstaged-changes "just regenerate-sqlx"
    echo "✅ {{BOLD}}check-rust done{{NORMAL}}"

# Run fast and simple Flutter lints.
@check-flutter:
    just _check-status "git lfs --version"
    just _check-unstaged-changes "git --no-pager diff"
    just _check-unstaged-changes "just flutter pub get"
    just _check-unstaged-changes "just dart format ."
    just _check-status "just flutter analyze --no-pub"
    just _check-unstaged-changes "just regenerate-l10n"
    just _check-unstaged-changes "just regenerate-icons"
    echo "✅ {{BOLD}}check-flutter done{{NORMAL}}"

# Run flutter rust bridge lint.
@check-frb:
    just _check-unstaged-changes "just regenerate-frb"

# Run all fast and simple lints.
@check: check-rust check-flutter check-frb

# This task will run the command. If the command fails, the task fails.
_check-status command:
    #!/usr/bin/env -S bash -eu
    echo "{{BOLD}}Running {{command}}{{NORMAL}}"
    if ! { {{command}}; }; then
        just _log-error "{{command}}"
    fi

# This task will run the command. If git diff then reports unstaged changes, the task will fail.
_check-unstaged-changes command:
    #!/usr/bin/env -S bash -eu
    echo "{{BOLD}}Running {{command}}{{NORMAL}}"
    {{command}}
    if ! git diff --quiet; then
        echo -e "{{RED}}Found unstaged changes.{{NORMAL}}"
        git --no-pager diff
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
regenerate: regenerate-frb regenerate-l10n regenerate-sqlx regenerate-icons

# Regenerate flutter rust bridge files.
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
    cd app && just flutter gen-l10n

# Regenerate database query metadata.
regenerate-sqlx: regenerate-sqlx-client regenerate-sqlx-server

# Regenerate client database query metadata.
[working-directory: 'coreclient']
regenerate-sqlx-client:
    cargo sqlx database setup --no-dotenv --database-url {{CLIENT_DATABASE_URL}}
    cargo sqlx prepare --no-dotenv --database-url {{CLIENT_DATABASE_URL}}

# Regenerate server database query metadata.
[working-directory: 'backend']
regenerate-sqlx-server: start-docker-compose
    cargo sqlx database setup --no-dotenv --database-url {{SERVER_DATABASE_URL}}
    cargo sqlx prepare --no-dotenv --database-url {{SERVER_DATABASE_URL}} -- --tests

# Recompile svg icons for rendering.
[working-directory: 'app']
regenerate-icons:
    just dart run tool/compile_svg_icons.dart

# Run cargo build, clippy and test.
@test-rust: start-docker-compose
    just _check-status "cargo clippy --locked --all-targets"
    just _check-status "cargo test --locked"
    echo "{{BOLD}}test-rust done{{NORMAL}}"

# Run flutter test.
test-flutter:
    cd app && just flutter test
    @echo "{{BOLD}}test-flutter done{{NORMAL}}"

# Run all lints and tests.
ci: check test

# Run all tests.
test: test-rust test-flutter

docker-is-podman := if `command -v podman || true` =~ ".*podman$" { "true" } else { "false" }
skip_docker := env_var_or_default("SKIP_DOCKER_COMPOSE", "false")
# Run docker compose services in the background.
@start-docker-compose: _generate-db-certs
    if [ "{{skip_docker}}" = "true" ]; then \
        echo "SKIP_DOCKER_COMPOSE is set, skipping docker compose"; \
    elif {{docker-is-podman}} == "true"; then \
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
    cd app && just flutter test --update-goldens

# Start the app in debug mode.
run-app *args='':
    cd app && just flutter run {{args}}

# Start the app from the last debug build.
run-app-cached:
    if [ "{{os()}}" = "windows" ]; then \
        app/build/windows/x64/runner/Debug/air.exe; \
    else \
        app/build/macos/Build/Products/Debug/Air.app/Contents/MacOS/Air; \
    fi

# Start the server.
run-server:
    cargo run --bin airserver | bunyan

# Increment minor version numbers and update changelog.
bump-version:
    cargo xtask bump-version

# Install fvm.
install-fvm:
    # If this fails, call this to get the new sha256sum:
    #  curl -fsSL https://fvm.app/install.sh -o install-fvm.sh
    #  sha256sum install-fvm.sh

    curl -fsSL https://fvm.app/install.sh -o install-fvm.sh
    bash install-fvm.sh 4.0.5

[working-directory: 'app']
build platform:
    if [[ "${CI:-false}" != "true" ]]; then just flutter build {{ platform }}; fi

[linux]
[working-directory: 'app/linux']
build-rpm: (build "linux")
    nfpm package -p rpm

[linux]
[working-directory: 'app/linux']
build-deb: (build "linux")
    nfpm package -p deb
