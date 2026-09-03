// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// Keyboard focus parked while a modal is up.
///
/// Makes it possible to restore the focus of a node which has the primary
/// focus, but only if the virtual keyboard was shown.
///
/// Call [suspend] before pushing the modal to drop that focus and [restore]
/// once the modal is gone to bring the keyboard back only if it was showing
/// before.
class SuspendedKeyboardFocus {
  SuspendedKeyboardFocus._(this._node);

  /// The field to refocus, null when the keyboard was not showing.
  final FocusNode? _node;

  static SuspendedKeyboardFocus suspend(BuildContext context) {
    final node = FocusManager.instance.primaryFocus;
    final keyboardShown = View.of(context).viewInsets.bottom > 0;
    node?.unfocus();
    return SuspendedKeyboardFocus._(keyboardShown ? node : null);
  }

  void restore() {
    final node = _node;
    // Gone from the tree, e.g. the chat was left while the modal was up.
    if (node == null || node.context == null || !node.canRequestFocus) return;
    node.requestFocus();
  }
}
