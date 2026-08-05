// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a menu card, per density.
///
/// Geometry only: colors come from the palette and the label style from the
/// typescale at paint time.
@immutable
class MenuTokens {
  const MenuTokens({
    required this.radius,
    required this.minWidth,
    required this.padding,
    required this.itemGap,
    required this.iconSize,
  });

  final double radius;

  /// Floor for the card's width. A menu of short labels still reads as a menu
  /// rather than as a sliver next to its trigger.
  final double minWidth;

  /// Inset from the card's edge to the row highlights.
  final EdgeInsets padding;

  final double itemGap;

  final double iconSize;

  /// Inset inside a row, so the highlight sits around the icon and label
  /// instead of flush against them.
  static const EdgeInsets itemPadding = EdgeInsets.symmetric(
    horizontal: S.s12,
    vertical: S.s8,
  );

  /// Corner radius of a row's hover, press, and selected highlight.
  static const double itemRadius = CornerRadius.px8;

  static const double iconGap = S.s8;

  static const double separatorWidth = StrokeWidth.px0_5;

  /// Space above and below a separator.
  static const double separatorGap = S.s4;

  /// Space between the card and a submenu opened beside it.
  static const double submenuGap = S.s4;

  static const Elevation elevation = Elevation.large;

  static const MenuTokens phone = MenuTokens(
    radius: CornerRadius.px20,
    minWidth: S.s192,
    padding: EdgeInsets.all(S.s12),
    itemGap: S.s4,
    iconSize: S.s20,
  );

  /// Denser than [phone]: a pointer hits a smaller row reliably, so the card
  /// tightens up and the glyphs shrink with it.
  static const MenuTokens desktop = MenuTokens(
    radius: CornerRadius.px12,
    minWidth: S.s160,
    padding: EdgeInsets.all(S.s8),
    itemGap: S.s2,
    iconSize: S.s12,
  );

  static MenuTokens get current => DeviceType.isPhone ? phone : desktop;
}
