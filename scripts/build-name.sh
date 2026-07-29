#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

# Prints the app version (the flutter build name).
#
# Release branches ship the version committed in app/pubspec.yaml, with the
# patch level managed by `just bump-version --patch`. Everywhere else (main,
# PR branches) the patch level is replaced at build time by the commit
# count, e.g. 0.19.1203, so successive builds of the same minor version are
# distinguishable.
set -eu

cd "$(dirname "$0")/.."

version=$(sed -n 's/^version: *\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' app/pubspec.yaml)
if [ -z "$version" ]; then
    echo "error: no version found in app/pubspec.yaml" >&2
    exit 1
fi

case "${GITHUB_REF_NAME:?build names are only available on CI}" in
release/*)
    echo "$version"
    ;;
*)
    echo "${version%.*}.$(git rev-list --count HEAD)"
    ;;
esac
