// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

/// States why a jump was requested:
///
/// - [quotedMessage]: navigating to a message that was quoted
/// - [firstUnread]: landing on the first unread message
enum JumpIntent { quotedMessage, firstUnread }

/// Emitted on [AnchoredListController.jumpedToId] when a jump-to-id
/// scroll completes successfully.
class JumpedToEvent {
  const JumpedToEvent(this.id, this.intent);
  final Object id;
  final JumpIntent intent;
}

/// Command types the controller queues for the widget to execute.
///
/// Only one command is pending at a time — issuing a new command replaces
/// any unprocessed one. The widget drains it via [AnchoredListController.drainCommand].
sealed class AnchoredListCommand {
  const AnchoredListCommand();
}

class GoToIdCommand extends AnchoredListCommand {
  const GoToIdCommand(this.id, this.intent);
  final Object id;
  final JumpIntent intent;
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
///  - **Observation** — [isAtBottom], [newestVisibleId], [oldestVisibleId],
///    and [isOldestVisibleHoisted] expose live viewport state via
///    [ValueListenable]s so callers can react without polling (e.g. showing
///    a "jump to latest" button or a floating date header anchored to the
///    top of the viewport).
class AnchoredListController extends ChangeNotifier {
  final ValueNotifier<bool> isAtBottomNotifier = ValueNotifier<bool>(true);
  final ValueNotifier<Object?> newestVisibleIdNotifier = ValueNotifier<Object?>(
    null,
  );
  final ValueNotifier<Object?> oldestVisibleIdNotifier = ValueNotifier<Object?>(
    null,
  );
  final ValueNotifier<bool> isOldestVisibleHoistedNotifier =
      ValueNotifier<bool>(false);
  ScrollController? scrollController;
  AnchoredListCommand? _pendingCommand;
  final StreamController<JumpedToEvent> _jumpedToIdController =
      StreamController<JumpedToEvent>.broadcast();

  /// Whether the list is currently at or near the bottom.
  ValueListenable<bool> get isAtBottom => isAtBottomNotifier;

  /// The newest currently visible item ID, if known.
  ///
  /// In a chat-style reversed list this is the item visually at the bottom
  /// of the viewport.
  ValueListenable<Object?> get newestVisibleId => newestVisibleIdNotifier;

  /// The oldest currently visible item ID, if known.
  ValueListenable<Object?> get oldestVisibleId => oldestVisibleIdNotifier;

  /// True when the oldest visible item's top edge has reached or passed
  /// `AnchoredList.oldestVisibleTopThreshold` (or `topPadding` if the
  /// threshold is not set).
  ValueListenable<bool> get isOldestVisibleHoisted =>
      isOldestVisibleHoistedNotifier;

  /// The current scroll position, if attached.
  ScrollPosition? get position =>
      scrollController?.hasClients == true ? scrollController!.position : null;

  /// The newest currently visible item ID, if attached.
  Object? get currentNewestVisibleId => newestVisibleIdNotifier.value;

  /// The oldest currently visible item ID, if attached.
  Object? get currentOldestVisibleId => oldestVisibleIdNotifier.value;

  /// Navigate to the message with [id].
  ///
  /// The widget decides the scroll strategy: if the item is already
  /// visible it animates smoothly, otherwise it jumps instantly.
  /// If not loaded, triggers `onLoadAround`.
  ///
  /// [intent] describes the situation triggering the jump and is
  /// forwarded on [jumpedToId] so consumers can react accordingly.
  void goToId(Object id, {required JumpIntent intent}) {
    _pendingCommand = GoToIdCommand(id, intent);
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

  /// Fires after a jump-to-id scroll has completed.
  Stream<JumpedToEvent> get jumpedToId => _jumpedToIdController.stream;

  /// Called by the widget when a jump-to-id scroll completes.
  void notifyJumpedToId(Object id, {required JumpIntent intent}) =>
      _jumpedToIdController.add(JumpedToEvent(id, intent));

  @override
  void dispose() {
    _jumpedToIdController.close();
    isAtBottomNotifier.dispose();
    newestVisibleIdNotifier.dispose();
    oldestVisibleIdNotifier.dispose();
    isOldestVisibleHoistedNotifier.dispose();
    super.dispose();
  }
}
