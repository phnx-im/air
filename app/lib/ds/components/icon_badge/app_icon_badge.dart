// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/icon_badge/app_icon_badge_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// An [AppIcon] on a rounded-rectangle plate.
///
/// [size] is the glyph's, not the badge's: the plate insets itself around the
/// glyph, so a host picks the size it wants the icon read at and the badge
/// grows with it.
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
    final palette = SemanticPalette.of(context);

    return Container(
      padding: AppIconBadgeTokens.padding(size),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: backgroundColor ?? palette.backgroundBase.tertiary,
        borderRadius: BorderRadius.circular(AppIconBadgeTokens.radius),
      ),
      child: AppIcon(type: type, size: size),
    );
  }
}
