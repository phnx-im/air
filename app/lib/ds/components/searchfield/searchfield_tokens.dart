// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a search field, per density.
///
/// Geometry only: colors come from the palette and text styles from the
/// typescale at paint time.
@immutable
class SearchFieldTokens {
  const SearchFieldTokens({
    required this.radius,
    required this.padding,
    required this.iconSize,
    required this.gap,
    required this.clearSize,
  });

  final double radius;

  /// Inset between the pill's edge and its content.
  final EdgeInsets padding;

  /// The leading search glyph.
  final double iconSize;

  /// Gap between the glyphs and the text.
  final double gap;

  /// The trailing clear glyph.
  final double clearSize;

  static const SearchFieldTokens phone = SearchFieldTokens(
    radius: CornerRadius.full,
    padding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    iconSize: S.s16,
    gap: S.s8,
    clearSize: S.s16,
  );

  /// Denser than [phone]: the glyphs shrink with the text around them so the
  /// pill stays in proportion at desktop density.
  static const SearchFieldTokens desktop = SearchFieldTokens(
    radius: CornerRadius.full,
    padding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s8),
    iconSize: S.s12,
    gap: S.s8,
    clearSize: S.s12,
  );

  static SearchFieldTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
