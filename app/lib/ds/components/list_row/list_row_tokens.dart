// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a list row, per density.
///
/// Geometry only: colors come from the palette at paint time, and the fill
/// a tile-shaped row carries is the host's to supply.
@immutable
class ListRowTokens {
  const ListRowTokens({
    required this.height,
    required this.padding,
    required this.leadingGap,
    required this.trailingGap,
    required this.sublabelGap,
    required this.separatorWidth,
    required this.radius,
  });

  /// Floor for the row's height, not a fixed one: a wrapping label or a tall
  /// leading slot grows the row rather than getting clipped by it.
  final double height;

  /// Inset between the row's edge and its content. The row owns it rather than
  /// the group around it, so a row standing on its own is inset the same as one
  /// in a run.
  final EdgeInsets padding;

  final double leadingGap;
  final double trailingGap;
  final double sublabelGap;

  final double separatorWidth;

  /// Corner radius of the filled tile. The separator look stays square, so it
  /// only applies to a row that carries a fill.
  final double radius;

  static const ListRowTokens phone = ListRowTokens(
    height: S.s56,
    padding: EdgeInsets.symmetric(horizontal: S.s16),
    leadingGap: S.s12,
    trailingGap: S.s8,
    sublabelGap: S.s4,
    separatorWidth: StrokeWidth.px0_5,
    radius: CornerRadius.px12,
  );

  /// Denser than [phone]: a pointer hits a smaller target reliably, so the
  /// two-pane layout fits more rows in the same column.
  static const ListRowTokens desktop = ListRowTokens(
    height: S.s48,
    padding: EdgeInsets.symmetric(horizontal: S.s16),
    leadingGap: S.s12,
    trailingGap: S.s8,
    sublabelGap: S.s4,
    separatorWidth: StrokeWidth.px0_5,
    radius: CornerRadius.px12,
  );

  static ListRowTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
