// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:flutter/material.dart';

enum MessageStatusIconType { sent, delivered, read }

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

    final iconType = switch (statusIcon) {
      MessageStatusIconType.sent => AppIconType.check,
      MessageStatusIconType.delivered => AppIconType.checkCheck,
      MessageStatusIconType.read => AppIconType.checkCheckFill,
    };

    return AppIcon(type: iconType, size: size, color: colors.text.tertiary);
  }
}
