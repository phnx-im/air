// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_meta/message_meta_tokens.dart';
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
    required this.isNewestOwn,
    required this.startsMessageGroup,
    required this.endsMessageGroup,
  });

  final bool isConnectionChat;

  /// Whether the tile plays the entrance animation. Read once, on mount.
  final bool animated;

  /// The newest message in the chat.
  final bool isNewest;

  /// The newest message the user sent.
  final bool isNewestOwn;

  /// The oldest message of a run the list shows as one block.
  final bool startsMessageGroup;

  /// The newest message of that run.
  final bool endsMessageGroup;

  @override
  Widget build(BuildContext context) {
    final userId = context.select((UserCubit cubit) => cubit.state.userId);
    final (
      messageId,
      message,
      inReplyToMessage,
      timestamp,
      status,
      reactions,
    ) = context.select(
      (MessageCubit cubit) => (
        cubit.state.message.id,
        cubit.state.message.message,
        cubit.state.message.inReplyToMessage,
        cubit.state.message.timestamp,
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

    final rowTokens = MessageRowTokens.current;
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
            // wide one where the sender changes, a tight one inside a group.
            padding: EdgeInsets.only(
              top: startsMessageGroup
                  ? rowTokens.groupGap
                  : MessageRowTokens.messageGap,
            ),
            child: TextMessageTile(
              messageId: messageId,
              contentMessage: content,
              inReplyToMessage: inReplyToMessage,
              timestamp: timestamp,
              startsMessageGroup: startsMessageGroup,
              endsMessageGroup: endsMessageGroup,
              status: adjustedStatus,
              isSender: isSender,
              showSender: !isConnectionChat,
              reactions: reactions,
              ownUserId: userId,
              isNewest: isNewest,
              isNewestOwn: isNewestOwn,
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

    return _AnimatedMessage(
      animate: animated,
      endsMessageGroup: endsMessageGroup,
      isSender: isSender,
      child: tile,
    );
  }
}

class _AnimatedMessage extends StatefulWidget {
  const _AnimatedMessage({
    required this.animate,
    required this.endsMessageGroup,
    required this.isSender,
    required this.child,
  });

  /// Honored on mount only. See [MessageRowContainer.animated].
  final bool animate;

  final bool endsMessageGroup;
  final bool isSender;
  final Widget child;

  @override
  State<_AnimatedMessage> createState() => _AnimatedMessageState();
}

class _AnimatedMessageState extends State<_AnimatedMessage>
    with SingleTickerProviderStateMixin {
  AnimationController? _controller;

  @override
  void initState() {
    super.initState();
    if (!widget.animate) return;
    _controller = AnimationController(
      vsync: this,
      duration: Effect.duration(MotionPreset.short),
    )..forward();
  }

  @override
  void dispose() {
    _controller?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final controller = _controller;
    if (controller == null) return widget.child;

    final reserve = widget.endsMessageGroup
        ? MessageMetaTokens.heightOf(context)
        : S.s0;

    final animation = CurvedAnimation(
      parent: controller,
      curve: Effect.easeOutQuart,
    );

    return Container(
      constraints: BoxConstraints(minHeight: reserve),
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
