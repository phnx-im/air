// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

/// Command types the controller queues for the widget to execute.
///
/// Only one command is pending at a time — issuing a new command replaces
/// any unprocessed one. The widget drains it via [AnchoredListController.drainCommand].
sealed class AnchoredListCommand {
  const AnchoredListCommand();
}

class GoToIdCommand extends AnchoredListCommand {
  const GoToIdCommand(this.id);
  final Object id;
}

class ScrollToBottomCommand extends AnchoredListCommand {
  const ScrollToBottomCommand({this.duration, this.curve});
  final Duration? duration;
  final Curve? curve;
}

/// External controller for [AnchoredList].
///
/// Provides the following capabilities:
///  - **Commands** — [goToId] and [scrollToBottom] queue a command that the
///    widget processes on the next frame.
///  - **Observation** — [isAtBottom] and [newestVisibleId] expose live
///    viewport state via [ValueListenable]s so callers can react without
///    polling (e.g. showing a "jump to latest" button).
class AnchoredListController extends ChangeNotifier {
  final ValueNotifier<bool> isAtBottomNotifier = ValueNotifier<bool>(true);
  final ValueNotifier<Object?> newestVisibleIdNotifier = ValueNotifier<Object?>(
    null,
  );
  ScrollController? scrollController;
  AnchoredListCommand? _pendingCommand;

  /// Whether the list is currently at or near the bottom.
  ValueListenable<bool> get isAtBottom => isAtBottomNotifier;

  /// The newest currently visible item ID, if known.
  ValueListenable<Object?> get newestVisibleId => newestVisibleIdNotifier;

  /// The current scroll position, if attached.
  ScrollPosition? get position =>
      scrollController?.hasClients == true ? scrollController!.position : null;

  /// The newest currently visible item ID, if attached.
  Object? get currentNewestVisibleId => newestVisibleIdNotifier.value;

  /// Navigate to the message with [id].
  ///
  /// The widget decides the scroll strategy: if the item is already
  /// visible it animates smoothly, otherwise it jumps instantly.
  /// If not loaded, triggers `onLoadAround`.
  void goToId(Object id) {
    _pendingCommand = GoToIdCommand(id);
    notifyListeners();
  }

  /// Scroll to the newest message (bottom).
  void scrollToBottom({Duration? duration, Curve? curve}) {
    _pendingCommand = ScrollToBottomCommand(duration: duration, curve: curve);
    notifyListeners();
  }

  /// Called by the widget to consume the pending command.
  AnchoredListCommand? drainCommand() {
    final cmd = _pendingCommand;
    _pendingCommand = null;
    return cmd;
  }

  @override
  void dispose() {
    isAtBottomNotifier.dispose();
    newestVisibleIdNotifier.dispose();
    super.dispose();
  }
}
