<!--
SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>

SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Air

This repository contains the code for a Rust implementation of the Air
Protocol, an infrastructure protocol built around the Messaging Layer Security
(MLS) group messaging protocol.

The Air Protocol aims to enable functionality commonly required by messaging
applications while providing strong security and privacy guarantees. It allows
federation between different servers and their connected clients.

For security, the protocol relies mainly on the strong security guarantees
provided by the underlying MLS protocol. The authentication service required by
the MLS protocol is a simple signature-based PKI.

## Code structure

The implementation spans client, server and test-specific components. It is
split across multiple crates:

- `backend`: Implements both the local and the federation part of the protocol
  logic on the server side. Inspired by the type-based verification design of
  the OpenMLS crate (on which both `coreclient` and `backend` are built),
  verification of incoming messages is enforced using Rust's type system. The
  `backend` is written to work with arbitrary storage providers. The crate
  itself provides an in-memory and a Postgres-based storage provider.
- `server`: The server component that makes the logic implemented in the
  `backend` available to clients via a REST API. Beside the REST API, the `server`
  also supports web-sockets, which the backend can use to notify clients of new
  messages. The `server` crate can be compiled into a binary that can be run to
  expose the HTTP endpoints. A `Dockerfile` is available to build a Docker image
  that contains the server binary.
- `coreclient`: Implements the protocol logic of the client component. The
  `coreclient` stores and manages a user's contacts, chats, as well as
  the underlying MLS groups. It provides a high-level API to make use of the
  protocol in the context of a messaging application. Just like the `backend`,
  the `coreclient` uses a type-based message verification approach. The crate
  can be used to instantiate and persist (via Sqlite) multiple clients
  parallely.
- `apiclient`: A shallow layer that the `coreclient` calls to interact with the
  `backend` via the `server` using gRPC.
- `applogic`: A layer using cubits to expose the functionality of the
  `coreclient` to a UI client.
- `app`: A UI client that uses `applogic` to provide a simple messaging
  application. The GUI is built using Flutter.
 - `common`: A shared library that contains the common code used by both the
  client and server.
- `test_harness`: Exclusively used for testing. The `test_harness` contains a
  test framework to conduct integration tests and can be compiled into a binary
  for [docker-based testing](#docker-based-federation-testing) of the protocol's
  federation capabilities.

## Development

<details>
<summary>Setup Instructions (macOS)</summary>

## Setup Instructions (macOS)

### Prerequisites

Before starting, ensure you have the following tools installed:

1. Clone the repository:

```bash
git clone https://github.com/phnx-im/air
```

2. Install [Rust](https://www.rust-lang.org/tools/install)

3. Install [Flutter SDK](https://docs.flutter.dev/get-started/install)

Verify your installation with:

```bash
flutter --version
```

> **Note:** installing Flutter through VS Code may run into problems when using the `just` setup scripts later. You may need to separately install Flutter outside of VS Code in order to follow the rest of these instructions.

4. Install required tools:

```bash
cargo install just flutter_rust_bridge_codegen sqlx-cli
```

- [`just`](https://github.com/casey/just): "is a handy way to save and run project-specific commands."
- [`flutter_rust_bridge_codegen`](https://github.com/fzyzcjy/flutter_rust_bridge): "Flutter/Dart <-> Rust binding generator"
- [`sqlx-cli`](https://github.com/launchbadge/sqlx): "SQLx's associated command-line utility for managing databases, migrations, and enabling "offline" mode with `sqlx::query!()` and friends."

5. Install [Docker Desktop on Mac](https://docs.docker.com/desktop/setup/install/mac-install/)

### Configuration Steps

1. Ensure that Docker is running. You can check your system tray or verify this on the CLI with:

```bash
docker info
```

> If you see something like `ERROR: Cannot connect to the Docker daemon at unix:///Users/[YOUR_USERNAME]/.docker/run/docker.sock. Is the docker daemon running?` then Docker is not running.

2. Initialize the database:

```bash
just init-db
```

> If you see the error `error getting credentials - err: exec: "docker-credential-desktop": executable file not found in $PATH`, then you should verify if you are running `docker-credential-osxkeychain` with the command `docker-credential-osxkeychain version`.
>
> If that works, then you will need to edit your `~/.docker/config.json`. Replace the value of `"credsStore"` with `"osxkeychain"`, then re-run `just init-db`.
>
> If you see the error `Error response from daemon: Ports are not available: exposing port TCP 127.0.0.1:5432 -> 0.0.0.0:0: listen tcp 127.0.0.1:5432: bind: address already in use`, verify that you are not already running Postgres on port 5432. Some users may be using popular apps like Postgres.app which runs on this port by default. Simply stop your server and try again.

3. Set up macOS requirements:

Install [Xcode](https://developer.apple.com/xcode/) and accept the license

```bash
sudo xcodebuild -license
```

Install [CocoaPods](https://guides.cocoapods.org/using/getting-started.html) (requires recent [Ruby](https://www.ruby-lang.org/en/documentation/installation/) version)

```
gem install cocoapods
```

### Running the App

Quick start:

```bash
cd app
flutter run -d macos
```

When prompted, use the domain name `localhost`.

</details>

## Licensing

All crates in this repository are licensed under the [AGPL
3.0](https://www.gnu.org/licenses/agpl-3.0.html). This README file is licensed
under [CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/).
