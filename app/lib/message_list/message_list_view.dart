// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';
import 'package:visibility_detector/visibility_detector.dart';

import 'chat_tile.dart';
import 'message_cubit.dart';
import 'message_list_cubit.dart';

typedef MessageCubitCreate =
    MessageCubit Function({
      required UserCubit userCubit,
      required MessageState initialState,
    });

class MessageListView extends StatefulWidget {
  const MessageListView({
    super.key,
    this.createMessageCubit = MessageCubit.new,
  });

  final MessageCubitCreate createMessageCubit;

  @override
  State<MessageListView> createState() => _MessageListViewState();
}

class _MessageListViewState extends State<MessageListView> {
  /// Holds the list of already animated messages
  ///
  /// This is used to prevent the animation from being triggered multiple times
  /// for messages that were newly added to the list, but are re-built because
  /// they were removed from the rendering tree due to the scroll position.
  final _animatedMessages = List<MessageId>.empty(growable: true);

  @override
  Widget build(BuildContext context) {
    final state = context.select((MessageListCubit cubit) => cubit.state);
    return ListView.custom(
      reverse: true,
      padding: EdgeInsets.only(
        top: kToolbarHeight + MediaQuery.of(context).padding.top,
      ),
      childrenDelegate: SliverChildBuilderDelegate(
        (context, reverseIndex) {
          final index = state.loadedMessagesCount - reverseIndex - 1;
          final message = state.messageAt(index);
          if (message == null) {
            return const SizedBox.shrink();
          }
          final animate =
              !_animatedMessages.contains(message.id) &&
              state.isNewMessage(message.id);
          if (animate) {
            _animatedMessages.add(message.id);
          }
          return BlocProvider(
            key: ValueKey(message.id),
            create: (context) {
              return widget.createMessageCubit(
                userCubit: context.read<UserCubit>(),
                initialState: MessageState(message: message),
              );
            },
            child: _VisibilityChatTile(
              messageId: message.id,
              timestamp: message.timestamp,
              child: ChatTile(
                isConnectionChat: state.isConnectionChat ?? false,
                animated: animate,
              ),
            ),
          );
        },
        findChildIndexCallback: (key) {
          final messageKey = key as ValueKey<MessageId>;
          final messageId = messageKey.value;
          final index = state.messageIdIndex(messageId);
          // reverse index
          return index != null ? state.loadedMessagesCount - index - 1 : null;
        },
        childCount: state.loadedMessagesCount,
      ),
    );
  }
}

class _VisibilityChatTile extends StatelessWidget {
  const _VisibilityChatTile({
    required this.messageId,
    required this.timestamp,
    required this.child,
  });

  final MessageId messageId;
  final DateTime timestamp;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final userCubit = context.read<UserCubit>();
    final chatDetailsCubit = context.read<ChatDetailsCubit>();
    return VisibilityDetector(
      key: ValueKey(VisibilityKeyValue(messageId)),
      child: child,
      onVisibilityChanged: (visibilityInfo) {
        if (visibilityInfo.visibleFraction > 0 &&
            userCubit.appState == AppState.foreground) {
          chatDetailsCubit.markAsRead(
            untilMessageId: messageId,
            untilTimestamp: timestamp,
          );
        }
      },
    );
  }
}

class VisibilityKeyValue {
  const VisibilityKeyValue(this.id);
  final MessageId id;

  @override
  int get hashCode => id.hashCode;

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is VisibilityKeyValue &&
            other.id == id);
  }
}
