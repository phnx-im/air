#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

username="${USERNAME:-dev}"
user_uid="$(id -u "$username")"
user_gid="$(id -g "$username")"

ensure_owned() {
    local path="$1"

    mkdir -p "$path"

    if [ "$(stat -c '%u:%g' "$path")" != "${user_uid}:${user_gid}" ]; then
        chown -R "${user_uid}:${user_gid}" "$path"
    fi
}

ensure_owned "/home/${username}/.cargo/registry"
ensure_owned "/home/${username}/.cargo/git"
ensure_owned "/workspace/target"

if [ "$#" -eq 0 ]; then
    set -- bash
fi

exec su -s /bin/bash "$username" -c 'exec "$0" "$@"' "$@"
