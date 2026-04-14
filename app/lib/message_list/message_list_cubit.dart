// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/anchored_list/data.dart';

class MessageListCubit extends StateStreamableSource<MessageListState> {
  MessageListCubit({required UserCubit userCubit, required ChatId chatId})
    : _impl = MessageListCubitBase(userCubit: userCubit.impl, chatId: chatId),
      _appliedRevision = 0 {
    _state = _impl.state;
    _appliedRevision = _state.meta.revision;
    _reloadMessageDataFromState(_state);
    _transitionSubscription = _impl.transitions().listen(_handleTransition);
    _stateSubscription = _impl.stream().listen((_) => _syncFromLatestState());
    _syncFromLatestState();
  }

  final MessageListCubitBase _impl;
  late final StreamSubscription<MessageListTransition> _transitionSubscription;
  late final StreamSubscription<MessageListState> _stateSubscription;
  final StreamController<MessageListState> _stateController =
      StreamController<MessageListState>.broadcast(sync: true);
  final StreamController<MessageListCommand> _commandController =
      StreamController<MessageListCommand>.broadcast(sync: true);
  late MessageListState _state;
  int _appliedRevision;
  int? _lastCommandRevision;

  final AnchoredListData<UiChatMessage> messageData = AnchoredListData();
  Stream<MessageListCommand> get commands => _commandController.stream;

  Future<void> loadOlder() => _impl.loadOlder();
  Future<void> loadNewer() => _impl.loadNewer();
  Future<void> jumpToBottom() => _impl.jumpToBottom();
  Future<void> jumpToMessage({required MessageId messageId}) =>
      _impl.jumpToMessage(messageId: messageId);

  void _handleTransition(MessageListTransition transition) {
    final revision = transition.revision;
    if (revision == _appliedRevision + 1) {
      _applyTransition(transition);
      _appliedRevision = revision;
      final latest = _impl.state;
      if (latest.meta.revision == revision) {
        _emitState(latest);
      }
    } else if (revision > _appliedRevision) {
      // Gap — skip incremental apply, let _syncFromLatestState handle it.
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
  }

  void _syncFromLatestState() {
    final latest = _impl.state;
    final revision = latest.meta.revision;
    if (revision <= _appliedRevision) return;
    _reloadMessageDataFromState(latest);
    _appliedRevision = revision;
    _emitState(latest);
  }

  void _reloadMessageDataFromState(MessageListState state) {
    final messages = <UiChatMessage>[];
    for (var i = state.loadedMessagesCount - 1; i >= 0; i--) {
      final message = state.messageAt(i);
      if (message != null) {
        messages.add(message);
      }
    }
    messageData.reload(messages);
  }

  void _emitState(MessageListState state) {
    _state = state;
    if (!_stateController.isClosed) {
      _stateController.add(state);
    }
  }

  void _emitCommand(MessageListCommand? command, int revision) {
    if (command == null || _lastCommandRevision == revision) return;
    _lastCommandRevision = revision;
    if (!_commandController.isClosed) {
      _commandController.add(command);
    }
  }

  @override
  FutureOr<void> close() async {
    await _transitionSubscription.cancel();
    await _stateSubscription.cancel();
    await _stateController.close();
    await _commandController.close();
    messageData.dispose();
    await _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  MessageListState get state => _state;

  @override
  Stream<MessageListState> get stream => _stateController.stream;
}
