// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

enum MessageStatusIconType { sent, delivered, read }

class RotatingSendIcon extends StatefulWidget {
  const RotatingSendIcon({super.key});

  @override
  State<RotatingSendIcon> createState() => _RotatingSendIconState();
}

class _RotatingSendIconState extends State<RotatingSendIcon>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      duration: const Duration(seconds: 1),
      vsync: this,
    )..repeat();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RotationTransition(
      turns: _controller,
      child: AppIcon.circleDashed(
        size: 16,
        color: CustomColorScheme.of(context).text.tertiary,
      ),
    );
  }
}

/// Renders the full message delivery status for a [UiMessageStatus].
///
/// Handles hidden, sending, error, sent, delivered, and read states,
/// respecting the user's read-receipts setting.
class MessageStatusIndicator extends StatefulWidget {
  const MessageStatusIndicator({super.key, required this.status});

  final UiMessageStatus status;

  @override
  State<MessageStatusIndicator> createState() => _MessageStatusIndicatorState();
}

class _MessageStatusIndicatorState extends State<MessageStatusIndicator> {
  Timer? _sendingTimer;
  bool _showSending = false;

  @override
  void initState() {
    super.initState();
    _updateSendingTimer();
  }

  @override
  void didUpdateWidget(MessageStatusIndicator oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.status != widget.status) {
      _updateSendingTimer();
    }
  }

  // Delays showing the sending spinner so that fast sends don't flash a
  // spinner. Cancelled if the status changes before firing.
  void _updateSendingTimer() {
    if (widget.status == UiMessageStatus.sending) {
      if (!_showSending && _sendingTimer == null) {
        _sendingTimer = Timer(const Duration(seconds: 2), () {
          _sendingTimer = null;
          setState(() => _showSending = true);
        });
      }
    } else {
      _sendingTimer?.cancel();
      _sendingTimer = null;
      _showSending = false;
    }
  }

  @override
  void dispose() {
    _sendingTimer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final readReceiptsEnabled = context.select(
      (UserSettingsCubit cubit) => cubit.state.readReceipts,
    );
    // Crossfades between children when their keys change.
    return AnimatedSwitcher(
      duration: const Duration(milliseconds: 150),
      child: _buildChild(context, readReceiptsEnabled),
    );
  }

  Widget _buildChild(BuildContext context, bool readReceiptsEnabled) {
    if (widget.status == UiMessageStatus.hidden) {
      return const SizedBox.shrink(key: ValueKey('hidden'));
    }
    if (widget.status == UiMessageStatus.sending) {
      if (!_showSending) {
        // Reserve icon-sized space to avoid layout jumps while waiting
        // for the delay to elapse.
        return const SizedBox.square(
          key: ValueKey('sendingHidden'),
          dimension: 16,
        );
      }
      return const RotatingSendIcon(key: ValueKey('sending'));
    }
    if (widget.status == UiMessageStatus.error) {
      return AppIcon.circleAlert(
        key: const ValueKey('error'),
        size: 16,
        color: CustomColorScheme.of(context).function.warning,
      );
    }
    final iconType = switch (widget.status) {
      UiMessageStatus.sent => MessageStatusIconType.sent,
      UiMessageStatus.delivered => MessageStatusIconType.delivered,
      UiMessageStatus.read =>
        readReceiptsEnabled
            ? MessageStatusIconType.read
            : MessageStatusIconType.delivered,
      _ => null,
    };
    if (iconType == null) {
      return const SizedBox.shrink(key: ValueKey('unknown'));
    }
    return MessageStatusIcon(
      key: ValueKey(iconType),
      size: 16,
      statusIcon: iconType,
    );
  }
}

class MessageStatusIcon extends StatelessWidget {
  const MessageStatusIcon({
    super.key,
    required this.statusIcon,
    this.size = 16,
  });

  final double size;
  final MessageStatusIconType statusIcon;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    final color = colors.text.tertiary;
    return switch (statusIcon) {
      MessageStatusIconType.sent => AppIcon.check(size: size, color: color),
      MessageStatusIconType.delivered => AppIcon.checkCheck(
        size: size,
        color: color,
      ),
      MessageStatusIconType.read => AppIcon.checkCheckFill(
        size: size,
        color: color,
      ),
    };
  }
}
