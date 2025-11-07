// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:provider/provider.dart';

import 'display_message_tile.dart';
import 'message_cubit.dart';
import 'text_message_tile.dart';

class ChatTile extends StatelessWidget {
  const ChatTile({
    super.key,
    required this.isConnectionChat,
    required this.animated,
  });

  final bool isConnectionChat;
  final bool animated;

  @override
  Widget build(BuildContext context) {
    final userId = context.select((UserCubit cubit) => cubit.state.userId);
    final (messageId, message, timestamp, position, status) = context.select(
      (MessageCubit cubit) => (
        cubit.state.message.id,
        cubit.state.message.message,
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
      duration: const Duration(milliseconds: 300),
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
      // Technically, this is the height of the timestampt and checkmark for the read message,
      // however the value is exactly the height + spacing.
      UiFlightPosition.single || UiFlightPosition.end => 27.0,
    };

    final animation = CurvedAnimation(
      parent: _controller,
      curve: Curves.easeOutQuart,
    );

    return Container(
      constraints: BoxConstraints(minHeight: fixedStartHeight),
      child: SizeTransition(
        axis: Axis.vertical,
        sizeFactor: animation,
        child: ScaleTransition(
          scale: animation,
          alignment:
              widget.isSender ? Alignment.bottomRight : Alignment.bottomLeft,
          child: widget.child,
        ),
      ),
    );
  }
}
