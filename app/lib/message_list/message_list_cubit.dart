// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/anchored_list/data.dart';

/// Bridges the Rust-side message list cubit ([MessageListCubitBase]) to
/// the Flutter widget layer.
///
/// The Rust side emits incremental diffs (insert, remove, update, reload)
/// in oldest-first order. This cubit translates each diff into the
/// reversed (newest-first) order that [AnchoredListData] expects, and
/// batches them into a single notification so the [AnchoredList] widget
/// sees one coherent update per state change.
///
/// ## Index conventions
///
/// Rust:          index 0 = oldest message
/// AnchoredList:  index 0 = newest message (matches the reversed scroll view)
///
/// Every diff index must be flipped before being applied to [messageData].
class MessageListCubit extends StateStreamableSource<MessageListState> {
  MessageListCubit({required UserCubit userCubit, required ChatId chatId})
    : _impl = MessageListCubitBase(userCubit: userCubit.impl, chatId: chatId) {
    _subscription = stream.listen(_applyDiffs);
    // Seed messageData with the initial state so the widget has content
    // on its first build, before any stream events arrive.
    _applyDiffs(state);
  }

  final MessageListCubitBase _impl;
  late final StreamSubscription<MessageListState> _subscription;

  /// The message list data with incremental diff tracking.
  ///
  /// Owned by this cubit, consumed by the [AnchoredList] widget.
  /// Index 0 is the newest message (reversed from the Rust-side ordering).
  final AnchoredListData<UiChatMessage> messageData = AnchoredListData();

  Future<void> loadOlder() => _impl.loadOlder();
  Future<void> loadNewer() => _impl.loadNewer();
  Future<void> jumpToBottom() => _impl.jumpToBottom();
  Future<void> jumpToMessage({required MessageId messageId}) =>
      _impl.jumpToMessage(messageId: messageId);

  /// Clears the one-shot scrollToIndex on the state so the view doesn't
  /// re-trigger the same scroll on the next rebuild.
  void clearScrollToIndex() => _impl.clearScrollToIndex();

  /// Drains pending diffs from the Rust side and applies them to
  /// [messageData], reversing indices along the way.
  ///
  /// All diffs within a single state emission are applied inside
  /// [AnchoredListData.batch] so the [AnchoredList] widget receives one
  /// combined notification and computes a single layout correction.
  void _applyDiffs(MessageListState state) {
    final diffs = _impl.drainMessageDiffs();
    if (diffs.isEmpty) return;

    messageData.batch(() {
      for (final diff in diffs) {
        switch (diff) {
          case MessageListDiff_Insert(:final index, :final messages):
            // Rust inserts at `index` (0 = oldest). In reversed order,
            // that maps to `length - index`. The items themselves are
            // also reversed so the newest lands at the lowest index.
            final reversedIndex = messageData.length - index.toInt();
            messageData.insertAll(reversedIndex, messages.reversed.toList());
          case MessageListDiff_Remove(:final index, :final count):
            // Rust's [index, index+count) range in oldest-first order
            // becomes [length-index-count, length-index) in newest-first.
            final len = messageData.length;
            final reversedIndex = len - index.toInt() - count.toInt();
            messageData.removeRange(reversedIndex, count.toInt());
          case MessageListDiff_Update(:final index, :final message):
            final reversedIndex = messageData.length - 1 - index.toInt();
            messageData.update(reversedIndex, message);
          case MessageListDiff_Reload(:final messages):
            // Full replacement — reverse the entire list.
            messageData.reload(messages.reversed.toList());
        }
      }
    });
  }

  @override
  FutureOr<void> close() {
    _subscription.cancel();
    messageData.dispose();
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  MessageListState get state => _impl.state;

  @override
  Stream<MessageListState> get stream => _impl.stream();
}
