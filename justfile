# SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set windows-shell := ["C:\\Program Files\\Git\\bin\\sh.exe","-c"]

# === Backend ===

POSTGRES_DATABASE_URL := "postgres://postgres:password@localhost:5432/air_db"

docker-is-podman := if `command -v podman || true` =~ ".*podman$" { "true" } else { "false" }

# run docker compose services in the background
run-services: generate-db-certs
    if {{docker-is-podman}} == "true"; then \
        podman rm air_minio-setup_1 -i 2>&1 /dev/null; \
        podman-compose --podman-run-args=--replace up -d; \
        podman-compose ps; \
        podman logs air_postgres_1; \
    else \
        docker compose up --wait --wait-timeout=300; \
        docker compose ps; \
    fi

# initialize the backend database and apply migrations
init-backend-db $DATABASE_URL=(POSTGRES_DATABASE_URL):
    cd backend && sqlx database create
    cd backend && sqlx database setup

[working-directory: 'backend']
prepare-db-statements $DATABASE_URL=(POSTGRES_DATABASE_URL):
    cargo sqlx prepare --database-url $DATABASE_URL -- --tests

# generate postgres TLS certificates
generate-db-certs:
    cd backend && TEST_CERT_DIR_NAME=test_certs scripts/generate_test_certs.sh

# === Client ===

[working-directory: 'coreclient']
init-client-db:
    sqlx database create --database-url sqlite://{{justfile_directory()}}/coreclient/client.db
    sqlx database setup --database-url sqlite://{{justfile_directory()}}/coreclient/client.db

[working-directory: 'coreclient']
prepare-client-db-statements: init-client-db
    cargo sqlx prepare --database-url sqlite://{{justfile_directory()}}/coreclient/client.db


# === App ===

app_lib_name := "applogic"
app_rust_base_dir := "../applogic"

# generate Dart files e.g. the data classes a.k.a freezed classes
[working-directory: 'app']
generate-dart-files:
    dart run build_runner build --delete-conflicting-outputs

# generate Rust and Dart flutter bridge files
[working-directory: 'app']
frb-generate $CARGO_TARGET_DIR=(justfile_directory() + "/target/frb_codegen"):
    rm -f {{app_rust_base_dir}}/src/frb_*.rs
    touch {{app_rust_base_dir}}/src/frb_generated.rs
    rm -Rf lib/core/api lib/core/frb_*.dart lib/core/lib.dart
    flutter_rust_bridge_codegen generate
    cd .. && cargo fmt

# Generate Rust and Dart flutter bridge files and check that they are committed
#
# Note: As a side effect, this recipe also checks whether the generated Dart
# files and the `app/pubspec.lock` file are up to date. This occurs because
# `flutter_rust_bridge_codegen` runs the `dart run build_runner build` command,
# which updates the generated files.
check-frb: frb-generate
    just check-clean-repo "just frb-generate"

# same as check-generated-frb (with all prerequisite steps for running in CI)
check-frb-ci: install-cargo-binstall
    cargo binstall flutter_rust_bridge_codegen@2.11.1 cargo-expand
    just check-frb

check-clean-repo command:
    #!/usr/bin/env -S bash -eu
    if [ -n "$(git status --porcelain)" ]; then
        git add -N .
        git --no-pager diff
        echo -e "\x1b[1;31mFound uncommitted changes. Did you forget to run '{{command}}'?"
        exit 1
    fi

# update the Flutter dependencies
[working-directory: 'app']
flutter-pub-get:
    flutter pub get

# check that the Flutter lockfile is up to date
[working-directory: 'app']
check-flutter-lockfile: flutter-pub-get
    # getting the Flutter dependencies may change the formatting
    just dart-format
    just check-clean-repo "just flutter-pub-get"

# format dart code
[working-directory: 'app']
dart-format:
    dart format .

# check that dart code is formatted
[working-directory: 'app']
check-dart-format: dart-format
    just check-clean-repo "just dart-format"

# generate localization files
[working-directory: 'app']
gen-l10n:
    flutter gen-l10n

# check that the localization files are up to date
[working-directory: 'app']
check-l10n: gen-l10n
    just check-clean-repo "just gen-l10n"

# set up the CI environment for the app
install-cargo-binstall:
    curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# set up the CI environment for Android builds
[working-directory: 'app/fastlane']
setup-android-ci: install-cargo-binstall
    cargo binstall -y cargo-ndk
    bundle install

# set up the CI environment for iOS builds
[working-directory: 'app/fastlane']
setup-ios-ci: install-cargo-binstall
    bundle install

# set up the CI environment for macOS builds
[working-directory: 'app/fastlane']
setup-macos-ci: install-cargo-binstall
    bundle install

test-rust *args='':
    env DATABASE_URL={{POSTGRES_DATABASE_URL}} SQLX_OFFLINE=true cargo test {{args}}

# build Android
# we limit it to android-arm64 to speed up the build process
[working-directory: 'app']
build-android:
     flutter build appbundle --target-platform android-arm64

# build iOS
[working-directory: 'app']
build-ios:
    flutter build ios --no-codesign

# Build Linux app
[working-directory: 'app']
build-linux:
     flutter build linux -v

# analyze Dart code
[working-directory: 'app']
analyze-dart:
    cd rust_builder/cargokit/build_tool && flutter pub get
    flutter analyze

# run Flutter tests
[working-directory: 'app']
test-flutter *args='':
    flutter test {{args}}

# run backend server (at localhost)
run-backend: init-backend-db
    cargo run --bin airserver

# Build Windows app
[working-directory: 'app']
build-windows:
     flutter build windows -v

# Run app
[working-directory: 'app']
run-app *args='':
    flutter run {{args}}

# Run app on Linux
run-app-linux *args='':
    just run-app -d linux {{args}}

# Add client migration
[working-directory: 'coreclient']
add-client-migration migration_name:
    sqlx migrate add {{migration_name}}
