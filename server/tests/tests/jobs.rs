// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// A PCO is created when executing a CO. The PCO remains after the CO execution
// only if the CO could not be completed successfully due to a network error or
// a wrong epoch error. Wrong epoch errors occur if another participant has also
// created a commit and the server has processed their commit first.
// Network errors can be simulated in a test by calling 

// Things to test
// - When executing a ChatOperation (CO) and there is a PendingChatOperation
//   (PCO) for the same chat, the PCO is executed first and then the CO.
// - When executing a CO and there is a PCO for the same chat, but the PCO
//   execution fails, the CO also fails and the PCO is not deleted.
// - When a PCO exists for a chat in state "waiting for queue response" and
//   we're getting a matching queue response, the pending commit should be
//   merged.
// - When a PCO exists for a chat in state "waiting for queue response" and
//   we're getting another commit for the same group, the following should
//   happen:
//   - If it's a leave operation, it should be deleted iff the incoming commit
//     covers that leave operation
//  - If it's not a leave operation, the incoming commit should be applied and
//    the existing pending commit should be discarded and the PCO should be
//    deleted.
// - When executing a PCO either as part of executing a CO or as part of the
//   retry mechanism, the following should happen:
//   - If the PCO is in the state "waiting for queue response", execution should
//     fail immediately.
//   - If the epoch is wrong because another participant has already committed
//     in the meantime, the PCO should be put into status "waiting for queue
//     response".
//   - If it's a network error, or this is a retry after an earlier network
//     error, check if the maximum retry count (5) was reached if it was, delete
//     the PCO.
//   - If the PCO is a leave operation, it should take immediate local effect
//     regardless of any "wrong epoch" or network errors. If there was such an
//     error, it should be retried, though.
//   - If the PCO was successfully executed, it should be deleted.