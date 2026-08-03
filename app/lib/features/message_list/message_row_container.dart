// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_row/message_row_tokens.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'package:air/features/message_list/display_message_tile.dart';
import 'package:air/features/message_list/message_cubit.dart';
import 'package:air/features/message_list/text_message_tile.dart';

class MessageRowContainer extends StatelessWidget {
  const MessageRowContainer({
    super.key,
    required this.isConnectionChat,
    required this.animated,
    required this.isNewest,
  });

  final bool isConnectionChat;

  /// Wraps the tile in [_AnimatedMessage] to play the entrance animation on
  /// mount. Rebuilds within the same mount preserve the controller's progress;
  /// flipping back to `false` (or never entering `true`) renders the tile
  /// directly.
  final bool animated;

  /// The newest message in the chat.
  final bool isNewest;

  @override
  Widget build(BuildContext context) {
    final userId = context.select((UserCubit cubit) => cubit.state.userId);
    final (
      messageId,
      message,
      inReplyToMessage,
      timestamp,
      position,
      status,
      reactions,
    ) = context.select(
      (MessageCubit cubit) => (
        cubit.state.message.id,
        cubit.state.message.message,
        cubit.state.message.inReplyToMessage,
        cubit.state.message.timestamp,
        cubit.state.message.position,
        cubit.state.message.status,
        cubit.state.message.reactions,
      ),
    );
    final isSender = switch (message) {
      UiMessage_Content(field0: final content) => content.sender == userId,
      UiMessage_Display() => false,
    };

    // Don't hide messages in blocked connection chats
    final adjustedStatus = switch (status) {
      UiMessageStatus.hidden when isConnectionChat => UiMessageStatus.sent,
      _ => status,
    };

    final rowTokens = MessageRowTokens.of(context);
    final isContent = message is UiMessage_Content;

    final tile = ListTile(
      // A message row insets itself. Only the event tiles still borrow the
      // list's own margin.
      contentPadding: isContent
          ? EdgeInsets.zero
          : const EdgeInsets.symmetric(horizontal: S.s16),
      dense: true,
      visualDensity: const VisualDensity(horizontal: 0, vertical: -4),
      minVerticalPadding: 0,
      title: Container(
        alignment: AlignmentDirectional.centerStart,
        child: switch (message) {
          UiMessage_Content(field0: final content) => Padding(
            // The list knows what precedes each row, so it owns the gaps: a
            // wide one where the sender changes, a tight one inside a flight.
            padding: EdgeInsets.only(
              top: position.isFirst
                  ? rowTokens.groupGap
                  : MessageRowTokens.flightGap,
            ),
            child: TextMessageTile(
              messageId: messageId,
              contentMessage: content,
              inReplyToMessage: inReplyToMessage,
              timestamp: timestamp,
              flightPosition: position,
              status: adjustedStatus,
              isSender: isSender,
              showSender: !isConnectionChat,
              reactions: reactions,
              ownUserId: userId,
              isNewest: isNewest,
            ),
          ),
          UiMessage_Display(field0: final display) => DisplayMessageTile(
            display,
            timestamp,
          ),
        },
      ),
      selected: false,
    );

    return animated
        ? _AnimatedMessage(position: position, isSender: isSender, child: tile)
        : tile;
  }
}

class _AnimatedMessage extends StatefulWidget {
  const _AnimatedMessage({
    required this.position,
    required this.isSender,
    required this.child,
  });

  final UiFlightPosition position;
  final bool isSender;
  final Widget child;

  @override
  State<_AnimatedMessage> createState() => _AnimatedMessageState();
}

class _AnimatedMessageState extends State<_AnimatedMessage>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: Effect.duration(MotionPreset.short),
    );
    _controller.forward();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final fixedStartHeight = switch (widget.position) {
      UiFlightPosition.start || UiFlightPosition.middle => 0.0,
      // FIXME: magic number
      // Technically, this is the height of the timestamp and checkmark for the read message,
      // however the value is exactly the height + spacing.
      UiFlightPosition.single || UiFlightPosition.end => 27.0,
    };

    final animation = CurvedAnimation(
      parent: _controller,
      curve: Effect.easeOutQuart,
    );

    return Container(
      constraints: BoxConstraints(minHeight: fixedStartHeight),
      child: SizeTransition(
        axis: Axis.vertical,
        sizeFactor: animation,
        child: ScaleTransition(
          scale: animation,
          alignment: widget.isSender
              ? Alignment.bottomRight
              : Alignment.bottomLeft,
          child: widget.child,
        ),
      ),
    );
  }
}
