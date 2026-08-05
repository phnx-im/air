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
    required this.itemPadding,
    required this.itemGap,
    required this.itemRadius,
    required this.iconSize,
    required this.iconGap,
    required this.separatorWidth,
    required this.separatorGap,
    required this.submenuGap,
    required this.elevation,
  });

  final double radius;

  /// Floor for the card's width. A menu of short labels still reads as a menu
  /// rather than as a sliver next to its trigger.
  final double minWidth;

  /// Inset from the card's edge to the row highlights.
  final EdgeInsets padding;

  /// Inset inside a row, so the highlight sits around the icon and label
  /// instead of flush against them.
  final EdgeInsets itemPadding;

  final double itemGap;

  /// Corner radius of a row's hover, press, and selected highlight.
  final double itemRadius;

  final double iconSize;
  final double iconGap;

  final double separatorWidth;

  /// Space above and below a separator.
  final double separatorGap;

  /// Space between the card and a submenu opened beside it.
  final double submenuGap;

  final Elevation elevation;

  static const MenuTokens phone = MenuTokens(
    radius: CornerRadius.px20,
    minWidth: S.s192,
    padding: EdgeInsets.all(S.s12),
    itemPadding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    itemGap: S.s4,
    itemRadius: CornerRadius.px8,
    iconSize: S.s20,
    iconGap: S.s8,
    separatorWidth: StrokeWidth.px0_5,
    separatorGap: S.s4,
    submenuGap: S.s4,
    elevation: Elevation.large,
  );

  /// Denser than [phone]: a pointer hits a smaller row reliably, so the card
  /// tightens up and the glyphs shrink with it.
  static const MenuTokens desktop = MenuTokens(
    radius: CornerRadius.px12,
    minWidth: S.s160,
    padding: EdgeInsets.all(S.s8),
    itemPadding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    itemGap: S.s2,
    itemRadius: CornerRadius.px8,
    iconSize: S.s12,
    iconGap: S.s8,
    separatorWidth: StrokeWidth.px0_5,
    separatorGap: S.s4,
    submenuGap: S.s4,
    elevation: Elevation.large,
  );

  static MenuTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
