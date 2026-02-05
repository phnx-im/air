// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// Things to test
// - Job execution with dependencies
// - Job execution when dependencies fail
// - Job execution when waiting for queue response
// - Job execution when DS request returns WrongEpochError
// - Job execution when DS request returns NetworkError
// - Job execution when DS request returns any other error
// - ChatOperation refinement
// - Check that operations fail when chat is inactive
// - Handling of Leave operations (immediate local removal and retries)
// - Check that a pending commit is successfully overwritten by a new commit
