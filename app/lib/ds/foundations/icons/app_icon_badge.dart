// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:flutter/material.dart';

/// An [AppIcon] wrapped in a rounded/circular background.
class AppIconBadge extends StatelessWidget {
  const AppIconBadge({
    super.key,
    required this.size,
    required this.type,
    this.backgroundColor,
  });

  final AppIconType type;
  final double size;
  final Color? backgroundColor;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Container(
      padding: EdgeInsets.all(size / 2),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: backgroundColor ?? colors.backgroundBase.tertiary,
        shape: BoxShape.rectangle,
        borderRadius: BorderRadius.circular(Spacing.px12),
      ),
      child: AppIcon(type: type, size: size),
    );
  }
}
