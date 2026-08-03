// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';

/// Bridges the message list (which owns the scroll controller) and the
/// composer (which shows the scroll-to-bottom button).
class ScrollToBottomController {
  final showButton = ValueNotifier<bool>(false);

  /// The current height of the overlaid composer, used by the message list
  /// to reserve matching bottom padding so content isn't obscured.
  final composerHeight = ValueNotifier<double>(0);

  VoidCallback? _onScrollToBottom;

  set onScrollToBottom(VoidCallback? callback) => _onScrollToBottom = callback;

  void scrollToBottom() => _onScrollToBottom?.call();

  void dispose() {
    showButton.dispose();
    composerHeight.dispose();
  }
}
