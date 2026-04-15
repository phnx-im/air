// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:collection';

import 'package:collection/collection.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/anchored_list/data.dart';
import 'package:logging/logging.dart';

final _log = Logger("MessageListCubit");

class MessageListStateWrapper {
  MessageListStateWrapper._({
    required this.state,
    required this.messageData,
    required Set<MessageId> loadedMessages,
    required Set<MessageId> newMessages,
  }) : loadedMessages = Set.unmodifiable(loadedMessages),
       newMessages = Set.unmodifiable(newMessages);

  factory MessageListStateWrapper.test({
    required MessageListState state,
    required AnchoredListData<UiChatMessage> messageData,
    Set<MessageId> loadedMessages = const {},
    Set<MessageId> newMessages = const {},
  }) => MessageListStateWrapper._(
    state: state,
    messageData: messageData,
    loadedMessages: loadedMessages,
    newMessages: newMessages,
  );

  final MessageListState state;
  final AnchoredListData<UiChatMessage> messageData; // reference, not value
  final Set<MessageId> loadedMessages;
  final Set<MessageId> newMessages;

  // Delegate Rust state fields
  bool get hasOlder => state.hasOlder;
  bool get hasNewer => state.hasNewer;
  bool get isAtBottom => state.isAtBottom;
  bool? get isConnectionChat => state.isConnectionChat;
  int? get firstUnreadIndex => state.firstUnreadIndex;

  // State queries
  bool isNewMessage(MessageId id) => newMessages.contains(id);
  bool isLoaded(MessageId id) => loadedMessages.contains(id);
  UiChatMessage? messageAt(int oldestFirstIndex) {
    final idx = messageData.length - oldestFirstIndex - 1;
    return (idx >= 0 && idx < messageData.length) ? messageData[idx] : null;
  }

  // TODO: Linear search
  UiChatMessage? messageById(MessageId id) =>
      messageData.items.firstWhereOrNull((m) => m.id == id);

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
    messageData = AnchoredListData();
    _state = MessageListStateWrapper._(
      state: _impl.state,
      messageData: messageData,
      loadedMessages: HashSet(),
      newMessages: HashSet(),
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

  // Cubit Data

  late MessageListStateWrapper _state;
  final Set<MessageId> _loadedMessages = HashSet<MessageId>();
  final Set<MessageId> _newMessages = HashSet<MessageId>();

  int _appliedRevision = 0;
  int? _lastCommandRevision;

  late final AnchoredListData<UiChatMessage> messageData;
  Stream<MessageListCommand> get commands => _commandController.stream;

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
    messageData.batch(() {
      for (final change in transition.changes) {
        switch (change) {
          case MessageListChange_Reload(:final messages):
            messageData.reload(messages);
            _loadedMessages.clear();
            _loadedMessages.addAll(messages.map((m) => m.id));
            _newMessages.clear();

          case MessageListChange_Splice(
            :final index,
            :final deleteCount,
            :final messages,
          ):
            final start = index.toInt();
            final count = deleteCount.toInt();
            if (count > 0) {
              for (var i = start; i < start + count; i++) {
                _loadedMessages.remove(messageData[i].id);
              }
              messageData.removeRange(start, count);
            }
            if (messages.isNotEmpty) {
              // Track entrance animations: new = not already loaded
              if (transition.kind ==
                  MessageListTransitionKind.newerPageLoaded) {
                for (final m in messages) {
                  if (!_loadedMessages.contains(m.id)) {
                    _newMessages.add(m.id);
                  }
                }
              }
              _loadedMessages.addAll(messages.map((m) => m.id));
              messageData.insertAll(start, messages);
            }
          case MessageListChange_Patch(:final index, :final message):
            messageData.update(index.toInt(), message);
        }
      }
    });
  }

  void _emitState(MessageListState state) {
    _state = MessageListStateWrapper._(
      state: state,
      messageData: messageData,
      loadedMessages: _loadedMessages, // takes a snapshot
      newMessages: _newMessages, // takes a snapshot
    );
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
    await _transitionSubscription.cancel();
    await _stateController.close();
    await _commandController.close();
    messageData.dispose();
    await _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  MessageListStateWrapper get state => _state;

  @override
  Stream<MessageListStateWrapper> get stream => _stateController.stream;
}
