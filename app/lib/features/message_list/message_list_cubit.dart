// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/anchored_list/data.dart';
import 'package:logging/logging.dart';

final _log = Logger("MessageListCubit");

class MessageListStateWrapper {
  MessageListStateWrapper({required this.state, required this.messageData});

  final MessageListState state;
  final AnchoredListData<UiChatMessage> messageData; // reference, not value

  // Delegate Rust state fields
  bool get hasOlder => state.hasOlder;
  bool get hasNewer => state.hasNewer;
  bool get isAtBottom => state.isAtBottom;
  bool? get isConnectionChat => state.isConnectionChat;
  int? get firstUnreadIndex => state.firstUnreadIndex;

  UiChatMessage? messageAt(int oldestFirstIndex) {
    final idx = messageData.length - oldestFirstIndex - 1;
    return (idx >= 0 && idx < messageData.length) ? messageData[idx] : null;
  }

  UiChatMessage? messageById(MessageId id) => messageData.itemById(id);

  /// Equality is based on the revision of the Rust state
  @override
  bool operator ==(Object other) =>
      other is MessageListStateWrapper &&
      state.revision == other.state.revision;

  @override
  int get hashCode => state.revision.hashCode;
}

class MessageListCubit extends StateStreamableSource<MessageListStateWrapper> {
  MessageListCubit({required UserCubit userCubit, required ChatId chatId})
    : _impl = MessageListCubitBase(userCubit: userCubit.impl, chatId: chatId) {
    messageData = AnchoredListData(idOf: (message) => message.id);
    _state = MessageListStateWrapper(
      state: _impl.state,
      messageData: messageData,
    );
    _appliedRevision = _state.state.revision;
    _transitionSubscription = _impl.transitions().listen(_handleTransition);
  }

  final MessageListCubitBase _impl;

  late final StreamSubscription<MessageListTransition> _transitionSubscription;
  final StreamController<MessageListStateWrapper> _stateController =
      StreamController<MessageListStateWrapper>.broadcast(sync: true);
  final StreamController<MessageListCommand> _commandController =
      StreamController<MessageListCommand>.broadcast(sync: true);
  final StreamController<Set<MessageId>> _incomingMessagesController =
      StreamController<Set<MessageId>>.broadcast(sync: true);

  // Cubit Data

  late MessageListStateWrapper _state;

  int _appliedRevision = 0;
  int? _lastCommandRevision;

  late final AnchoredListData<UiChatMessage> messageData;
  Stream<MessageListCommand> get commands => _commandController.stream;

  /// Emits each batch of messages that have just been appended to the
  /// newest end of the list (via a `NewerPageLoaded` transition) and
  /// weren't in the window before. The view subscribes to this to drive
  /// the entrance animation, gated by live scroll position at arrival.
  Stream<Set<MessageId>> get incomingMessages =>
      _incomingMessagesController.stream;

  // Public API

  Future<void> loadOlder() => _impl.loadOlder();
  Future<void> loadNewer() => _impl.loadNewer();
  Future<void> jumpToBottom() => _impl.jumpToBottom();
  Future<void> jumpToMessage({required MessageId messageId}) =>
      _impl.jumpToMessage(messageId: messageId);

  // Internal API

  void _handleTransition(MessageListTransition transition) {
    final revision = transition.revision;
    if (revision == _appliedRevision + 1) {
      _applyTransition(transition);
      _appliedRevision = revision;
      final latest = _impl.state;
      _emitState(latest);
    } else if (revision > _appliedRevision) {
      _log.severe(
        "Gap in message list revision: expected ${_appliedRevision + 1}, got $revision",
      );
      // Gap — skip incremental apply
      return;
    }
    if (revision == _appliedRevision) {
      _emitCommand(transition.command, revision);
    }
  }

  void _applyTransition(MessageListTransition transition) {
    if (transition.changes.isEmpty) return;
    // Checked before applying, since a message that moves within the window is
    // removed and reinserted by the same transition.
    final incoming = <MessageId>{
      if (transition.kind == MessageListTransitionKind.newerPageLoaded)
        for (final change in transition.changes)
          if (change case MessageListChange_Splice(:final messages))
            for (final m in messages)
              if (!messageData.containsId(m.id)) m.id,
    };
    messageData.batch(() {
      for (final change in transition.changes) {
        switch (change) {
          case MessageListChange_Reload(:final messages):
            messageData.reload(messages);

          case MessageListChange_Splice(
            :final index,
            :final deleteCount,
            :final messages,
          ):
            final start = index.toInt();
            final count = deleteCount.toInt();
            if (count > 0) {
              messageData.removeRange(start, count);
            }
            if (messages.isNotEmpty) {
              messageData.insertAll(start, messages);
            }
          case MessageListChange_Patch(:final index, :final message):
            messageData.update(index.toInt(), message);
        }
      }
    });
    if (incoming.isNotEmpty && !_incomingMessagesController.isClosed) {
      _incomingMessagesController.add(incoming);
    }
  }

  void _emitState(MessageListState state) {
    _state = MessageListStateWrapper(state: state, messageData: messageData);
    if (!_stateController.isClosed) {
      _stateController.add(_state);
    }
  }

  void _emitCommand(MessageListCommand? command, int revision) {
    if (command == null || _lastCommandRevision == revision) return;
    _lastCommandRevision = revision;
    if (!_commandController.isClosed) {
      _commandController.add(command);
    }
  }

  // Cubit interface

  @override
  FutureOr<void> close() async {
    // Trigger Rust-side cancellation first so the transitions() task exits
    // and drops its StreamSink. Awaiting the Dart subscription cancel before
    // this would deadlock: the FRB ReceivePort only closes once Rust sends
    // CloseStreamException via the dropped sink.
    await _impl.close();
    await _transitionSubscription.cancel();
    await _stateController.close();
    await _commandController.close();
    await _incomingMessagesController.close();
    messageData.dispose();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  MessageListStateWrapper get state => _state;

  @override
  Stream<MessageListStateWrapper> get stream => _stateController.stream;
}
