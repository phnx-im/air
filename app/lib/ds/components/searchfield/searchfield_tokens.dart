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
  const SearchFieldTokens({required this.iconSize, required this.clearSize});

  /// The leading search glyph.
  final double iconSize;

  /// The trailing clear glyph.
  final double clearSize;

  static const double radius = CornerRadius.full;

  /// Inset between the pill's edge and its content.
  static const EdgeInsets padding = EdgeInsets.symmetric(
    horizontal: S.s12,
    vertical: S.s8,
  );

  /// Gap between the glyphs and the text.
  static const double gap = S.s8;

  static const SearchFieldTokens phone = SearchFieldTokens(
    iconSize: S.s16,
    clearSize: S.s16,
  );

  /// Denser than [phone]: the glyphs shrink with the text around them so the
  /// pill stays in proportion at desktop density.
  static const SearchFieldTokens desktop = SearchFieldTokens(
    iconSize: S.s12,
    clearSize: S.s12,
  );

  static SearchFieldTokens get current => DeviceType.isPhone ? phone : desktop;
}
