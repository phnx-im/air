# SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set minimum-version := "1.56.0"
set default-list
set script-interpreter := ['bash', '-eu']

[windows]
set shell := ['C:\Program Files\Git\bin\sh.exe', '-c']
[windows]
set script-interpreter := ['C:\Program Files\Git\bin\bash.exe', '-eu']

export RUST_BACKTRACE := "1"
export RUSTFLAGS := "-D warnings"

ci := env("CI", "false")

POSTGRES_HOST := env("POSTGRES_HOST", "localhost")
SERVER_DATABASE_URL := "postgres://postgres:password@" + POSTGRES_HOST + ":5432/air_db"

[unix]
CLIENT_DATABASE_URL := "sqlite://" + justfile_directory() + "/coreclient/client.db"
[windows]
CLIENT_DATABASE_URL := "sqlite:///" + replace(justfile_directory(), "\\", "/") + "/coreclient/client.db"

# Run dart (via fvm outside of CI).
[working-directory('app')]
@dart *args:
    {{ if ci == "true" { "dart" } else { "fvm dart" } }} {{ args }}

# Run flutter (via fvm outside of CI).
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

# Check that generated l10n and icon files are up to date.
[group('check')]
check-app-resources: regenerate-l10n regenerate-icons && _check-unstaged-changes

# Lint Rust code with clippy.
[group('check')]
check-clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Check Rust dependencies for advisories, licenses and bans.
[group('check')]
check-cargo-deny:
    cargo deny fetch
    cargo deny check

# Check for unused Rust dependencies.
[group('check')]
check-cargo-machete:
    cargo machete --with-metadata

# Check Dart formatting and run the analyzer.
[group('check')]
check-dart:
    just flutter pub get
    just dart format . -o none --set-exit-if-changed
    just dart analyze --fatal-infos

# Check that generated flutter rust bridge files are up to date.
[group('check')]
check-frb: regenerate-frb && _check-unstaged-changes

# Check that the generated Rust licenses file is up to date.
[group('check')]
check-licenses: regenerate-licenses && _check-unstaged-changes

# Check the ARB files for problems gen-l10n accepts silently.
[group('check')]
check-l10n:
    cargo xtask validate-l10n

# Build, lint and check formatting of the protobuf files.
[group('check')]
[working-directory('protos')]
check-buf:
    buf build
    buf lint
    buf format --diff --exit-code
    # buf breaking --against '{{justfile_directory()}}/.git#branch=origin/main'

# Check SPDX license headers.
[group('check')]
check-reuse:
    reuse lint -l

# Check Rust formatting.
[group('check')]
check-rustfmt:
    cargo fmt -- --check

# This task will run the command. If git diff then reports unstaged changes, the task will fail.
[script]
_check-unstaged-changes:
    diff=$(git --no-pager diff)
    if [ -n "$diff" ]; then
        echo -e "{{RED}}Found unstaged changes.{{NORMAL}}"
        echo "$diff"
        exit 1
    fi

# Regenerate flutter rust bridge files.
[working-directory: 'app']
[group('regenerate')]
regenerate-frb:
    rm -f ../applogic/src/frb_*.rs
    touch ../applogic/src/frb_generated.rs
    rm -Rf lib/core/api lib/core/frb_*.dart lib/core/lib.dart

    CARGO_TARGET_DIR="{{justfile_directory()}}/target/frb_codegen" \
        flutter_rust_bridge_codegen generate --no-web

    just dart run build_runner build
    cd .. && cargo fmt

# Regenerate localization files.
[working-directory: 'app']
[group('regenerate')]
regenerate-l10n:
    cargo xtask prune-unused-l10n # pass --apply and optionally --safe to prevent data loss
    just flutter gen-l10n

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

# Regenerate the licenses of the Rust dependencies shipped in the app.
[group('regenerate')]
regenerate-licenses:
    cargo xtask generate-licenses

# Recompile svg icons for rendering.
[working-directory: 'app']
[group('regenerate')]
regenerate-icons:
    just dart run tool/compile_svg_icons.dart

# TZ is pinned because goldens depict clock labels in the host's zone. The tests
# don't load the Rust library, so skip the native assets hook, which would
# otherwise build it.

# Run flutter test.
[working-directory: 'app']
[env('TZ', 'UTC')]
[env('FLUTTER_NATIVE_ASSETS', 'false')]
test-flutter *args:
    just flutter test {{ args }}

skip_docker := env("SKIP_DOCKER_COMPOSE", "false")

# Run docker compose services in the background.
[script]
start-docker-compose:
    if [ "{{skip_docker}}" = "true" ]; then
        echo "SKIP_DOCKER_COMPOSE is set, skipping docker compose"
    else
        docker compose up --quiet-pull --remove-orphans --detach --wait --wait-timeout=60
    fi

# Use the current test results as new reference images.
[working-directory: 'app']
update-goldens:
    # Delete existing goldens, so that the ones whose test is gone do not
    # linger.
    git ls-files -z 'test/**/goldens/*.{{ os() }}.png' | xargs -0 rm -f
    # Update golden snapshots
    just test-flutter --update-goldens

# Trigger the "Update Goldens" workflow on the current branch, or a given PR.
[script]
update-goldens-ci pr='':
    ref=$(gh pr view "{{pr}}" --json headRefName -q .headRefName)
    echo "Dispatching update-goldens.yml on ref: $ref"
    gh workflow run update-goldens.yml --ref "$ref"

# Start the app in debug mode.
[working-directory: 'app']
run-app *args:
    just flutter run {{args}}

# Start the server.
run-server:
    cargo run --bin airserver | bunyan

# Print the store build number (workflow run number plus a fixed offset).
@build-number:
    bash scripts/build-number.sh

# Increment version numbers (minor by default, --patch on release branches).
bump-version *args:
    cargo xtask bump-version {{args}}

# Cut a release/0.X branch from main (at the given commit, default HEAD).
cut-release *args:
    cargo xtask cut-release {{args}}

# Install fvm.
install-fvm:
    # If this fails, call this to get the new sha256sum:
    #  curl -fsSL https://fvm.app/install.sh -o install-fvm.sh
    #  sha256sum install-fvm.sh

    curl -fsSL https://fvm.app/install.sh -o install-fvm.sh
    bash install-fvm.sh 4.0.5

# Build the app for the given platform (no-op in CI).
[working-directory: 'app']
build platform:
    if [[ "${CI:-false}" != "true" ]]; then fvm flutter build {{ platform }}; fi

app_flavor := env("APP_FLAVOR", "staging")

# Package the Linux build as an rpm.
[linux]
[working-directory: 'app/linux']
[env('APP_FLAVOR', app_flavor)]
build-rpm:
    nfpm package -p rpm

# Package the Linux build as a deb.
[linux]
[working-directory: 'app/linux']
[env('APP_FLAVOR', app_flavor)]
build-deb:
    nfpm package -p deb
