// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// Everything `NavItem` needs to paint itself, fully resolved by the host.
@immutable
class NavItemTokens {
  const NavItemTokens({
    required this.boxWidth,
    required this.boxHeight,
    required this.radius,
    required this.labelGap,
    required this.padding,
    required this.surface,
    required this.activeLabelStyle,
    required this.inactiveLabelStyle,
  });

  final double boxWidth;
  final double boxHeight;
  final double radius;

  /// Gap between the glyph and the label.
  final double labelGap;

  final EdgeInsets padding;

  /// The panel the item sits on. Picks the hover and press wash direction.
  final Color surface;

  final TextStyle activeLabelStyle;
  final TextStyle inactiveLabelStyle;
}
