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

[group('check')]
check-app-resources: regenerate-l10n regenerate-icons && _check-unstaged-changes

[group('check')]
check-clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

[group('check')]
check-cargo-deny:
    cargo deny fetch
    cargo deny check

[group('check')]
check-cargo-machete:
    cargo machete

[group('check')]
check-dart:
    just flutter pub get
    just dart format . -o none --set-exit-if-changed
    just dart analyze --fatal-infos

[group('check')]
check-frb: regenerate-frb && _check-unstaged-changes

[group('check')]
check-reuse:
    reuse lint -l

[group('check')]
check-rustfmt:
    cargo fmt -- --check

# This task will run the command. If git diff then reports unstaged changes, the task will fail.
_check-unstaged-changes:
    #!/usr/bin/env -S bash -eu
    if ! git diff --quiet; then
        echo -e "{{RED}}Found unstaged changes.{{NORMAL}}"
        git --no-pager diff
    fi

# Regenerate flutter rust bridge files.
[working-directory: 'app']
[group('regenerate')]
regenerate-frb:
    rm -f ../applogic/src/frb_*.rs
    touch ../applogic/src/frb_generated.rs
    rm -Rf lib/core/api lib/core/frb_*.dart lib/core/lib.dart

    CARGO_TARGET_DIR="{{justfile_directory()}}/target/frb_codegen" \
        flutter_rust_bridge_codegen generate

    cd .. && cargo fmt

# Regenerate localization files.
[group('regenerate')]
regenerate-l10n:
    cd app && cargo xtask prune-unused-l10n # pass --apply and optionally --safe to prevent data loss
    cd app && just flutter gen-l10n

# Regenerate database query metadata.
[group('regenerate')]
regenerate-sqlx: regenerate-sqlx-client regenerate-sqlx-server

# Regenerate client database query metadata.
[working-directory: 'coreclient']
[group('regenerate')]
regenerate-sqlx-client:
    cargo sqlx database setup --no-dotenv --database-url {{CLIENT_DATABASE_URL}}
    cargo sqlx prepare --no-dotenv --database-url {{CLIENT_DATABASE_URL}}

# Regenerate server database query metadata.
[working-directory: 'backend']
[group('regenerate')]
regenerate-sqlx-server: start-docker-compose
    cargo sqlx database setup --no-dotenv --database-url {{SERVER_DATABASE_URL}}
    cargo sqlx prepare --no-dotenv --database-url {{SERVER_DATABASE_URL}} -- --tests

# Recompile svg icons for rendering.
[working-directory: 'app']
[group('regenerate')]
regenerate-icons:
    just dart run tool/compile_svg_icons.dart

# Run cargo build, clippy and test.
@test-rust: start-docker-compose
    cargo clippy --locked --all-targets
    cargo test --locked
    just test-rust-apq-groups

# Run pre-compiled integration tests with APQ groups enabled
@test-rust-apq-groups:
    #!/usr/bin/env -S bash -eu
    # Note: This is such a complicated command to avoid recompilation of the
    # integration tests, which burns quite some time in CI.
    RUNNER=$(cargo test --no-run --message-format=json 2>/dev/null | jq -r 'select(.reason == "compiler-artifact" and .target.name == "integration") | .executable')
    echo "Running integration tests with APQ groups enabled: $RUNNER"
    env TEST_WITH_APQ_GROUPS=true $RUNNER

# Run flutter test.
test-flutter:
    cd app && just flutter test
    @echo "{{BOLD}}test-flutter done{{NORMAL}}"

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
update-goldens:
    cd app && just flutter test --update-goldens

# Trigger the "Update Goldens" workflow on the current branch, or a given PR.
[script]
update-goldens-ci pr='':
    ref=$(gh pr view "{{pr}}" --json headRefName -q .headRefName)
    echo "Dispatching update-goldens.yml on ref: $ref"
    gh workflow run update-goldens.yml --ref "$ref"

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
