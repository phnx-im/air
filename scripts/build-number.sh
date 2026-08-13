#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

# Prints the store build number: the workflow run number plus a fixed
# offset.
#
# All jobs of a workflow run share GITHUB_RUN_NUMBER, so every platform of
# one release run bakes in the same number. The release workflow's run
# number increases with every run regardless of branch, which keeps Google
# Play's versionCode monotonically increasing across main and release/*
# uploads; uploads are serialized in run order via the release workflow's
# concurrency queue. The offset keeps numbers above previously uploaded
# builds, whose numbers came from the git commit count (last: ~1203).
#
# PR builds bake in a number from their own workflow's run counter; those
# builds are never uploaded to a store.
set -eu

offset=1300
echo $((${GITHUB_RUN_NUMBER:?build numbers are only available on CI} + offset))
