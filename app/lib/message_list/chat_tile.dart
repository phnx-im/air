// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/effects/motion.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'display_message_tile.dart';
import 'message_cubit.dart';
import 'text_message_tile.dart';

class ChatTile extends StatelessWidget {
  const ChatTile({
    super.key,
    required this.isConnectionChat,
    required this.animated,
    this.shouldAnimate = false,
  });

  final bool isConnectionChat;

  /// Whether to wrap in [_AnimatedMessage] and keep widget tree stable.
  final bool animated;

  /// Tells us whether we should animate the message.
  final bool shouldAnimate;

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
    ) = context.select(
      (MessageCubit cubit) => (
        cubit.state.message.id,
        cubit.state.message.message,
        cubit.state.message.inReplyToMessage,
        cubit.state.message.timestamp,
        cubit.state.message.position,
        cubit.state.message.status,
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

    final tile = ListTile(
      contentPadding: const EdgeInsets.symmetric(horizontal: Spacings.s),
      dense: true,
      visualDensity: const VisualDensity(horizontal: 0, vertical: -4),
      minVerticalPadding: 0,
      title: Container(
        alignment: AlignmentDirectional.centerStart,
        child: switch (message) {
          UiMessage_Content(field0: final content) => TextMessageTile(
            messageId: messageId,
            contentMessage: content,
            inReplyToMessage: inReplyToMessage,
            timestamp: timestamp,
            flightPosition: position,
            status: adjustedStatus,
            isSender: isSender,
            showSender: !isConnectionChat,
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
        ? _AnimatedMessage(
            position: position,
            isSender: isSender,
            shouldAnimate: shouldAnimate,
            child: tile,
          )
        : tile;
  }
}

class _AnimatedMessage extends StatefulWidget {
  const _AnimatedMessage({
    required this.position,
    required this.isSender,
    required this.shouldAnimate,
    required this.child,
  });

  final UiFlightPosition position;
  final bool isSender;

  /// Tells us whether we should animate the message.
  final bool shouldAnimate;
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
      duration: motionShort,
      value: widget.shouldAnimate ? 0.0 : 1.0,
    );
    if (widget.shouldAnimate) {
      _controller.forward();
    }
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

    final animation = CurvedAnimation(parent: _controller, curve: motionEasing);

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
