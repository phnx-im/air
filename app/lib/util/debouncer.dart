// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:ui';

/// A simple task debouncer
class Debouncer {
  Debouncer({required this.delay});

  final Duration delay;
  Timer? _timer;
  VoidCallback? _pending;

  void run(VoidCallback action) {
    _timer?.cancel();
    _pending = action;
    _timer = Timer(delay, () {
      _pending = null;
      action();
    });
  }

  /// Runs a pending action right away instead of waiting out the delay.
  ///
  /// A no-op when nothing is pending. Use this on dispose when the action
  /// must not be dropped.
  void flush() {
    final pending = _pending;
    dispose();
    pending?.call();
  }

  void dispose() {
    _timer?.cancel();
    _pending = null;
  }
}
