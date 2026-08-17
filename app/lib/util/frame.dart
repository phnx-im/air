// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

/// Whether the tree is being built, laid out or painted right now, so marking
/// anything dirty would throw.
bool get isMidFrame =>
    SchedulerBinding.instance.schedulerPhase ==
    SchedulerPhase.persistentCallbacks;

/// Lets a state defer a mutation that rebuilds out of the running frame.
///
/// Scroll notifications are the usual reason to need this: a dimension change
/// during the viewport's own layout settles a ballistic activity, which
/// dispatches start and end notifications synchronously, from inside
/// `performLayout`.
mixin FrameSafeState<T extends StatefulWidget> on State<T> {
  /// Runs [mutation] now, or at the end of the frame already in flight.
  void runFrameSafe(VoidCallback mutation) {
    if (!isMidFrame) {
      mutation();
      return;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) mutation();
    });
  }
}
