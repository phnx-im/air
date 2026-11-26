// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Whom a [MentionPill] names.
enum MentionPillVariant {
  /// The reader.
  me,

  /// Anyone else.
  other,
}

/// The look of each [MentionPillVariant], resolved against the palette.
abstract final class MentionPillTokens {
  static const EdgeInsets padding = EdgeInsets.symmetric(horizontal: S.s2);

  /// A pill sets the height of its line. Its vertical space sits outside the
  /// fill, which leaves a gap between pills on consecutive lines.
  static const EdgeInsets margin = EdgeInsets.symmetric(vertical: S.s1);

  static const double radius = CornerRadius.px4;

  /// The dark fill doubles the alpha, because a light tint all but vanishes
  /// against a dark bubble.
  static const double fillAlphaLight = Alpha.a10;
  static const double fillAlphaDark = Alpha.a20;

  /// How far the dark-mode label moves toward white. The warning and link
  /// colors land mid-tone on a dark bubble and read poorly as text.
  static const double darkLabelBrighten = 0.45;

  static Color fill(SemanticPalette palette, MentionPillVariant variant) =>
      _base(palette, variant).withValues(
        alpha: palette.brightness == .dark ? fillAlphaDark : fillAlphaLight,
      );

  static Color label(SemanticPalette palette, MentionPillVariant variant) {
    final base = _base(palette, variant);
    if (palette.brightness == .light) return base;
    return Color.lerp(base, palette.function.neutral.white, darkLabelBrighten)!;
  }

  static Color _base(SemanticPalette palette, MentionPillVariant variant) =>
      switch (variant) {
        MentionPillVariant.me => palette.function.warning.primary,
        MentionPillVariant.other => palette.function.link,
      };
}
