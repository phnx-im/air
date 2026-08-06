// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the emoji menu, per density.
///
/// Geometry only: colors come from the palette and the glyph and label
/// styles from the typescale at paint time. The surface the menu sits on is the
/// host sheet's or panel's to supply.
@immutable
class ReactionEmojiMenuTokens {
  const ReactionEmojiMenuTokens({
    required this.headerHeight,
    required this.headerGap,
    required this.cellExtent,
    required this.cellGap,
    required this.fadeHeight,
    required this.flyoutPadding,
    required this.flyoutItemPadding,
    required this.flyoutHelpGap,
  });

  /// Height of the search field and of the square tone button beside it. The
  /// two share it so they line up.
  final double headerHeight;

  /// Gap between the header and the grid below it.
  final double headerGap;

  /// Ceiling for a cell's side. The grid fits as many whole cells across as the
  /// width allows, so a cell is at most this wide and usually a little less.
  final double cellExtent;
  final double cellGap;

  /// Height of the fade over each end of the grid.
  final double fadeHeight;

  /// Inset around the tone swatches in the flyout.
  final EdgeInsets flyoutPadding;

  /// Inset around one swatch, which widens its target beyond the glyph.
  final EdgeInsets flyoutItemPadding;

  /// Gap between the swatches and the caption under them.
  final double flyoutHelpGap;

  /// Gap between the search field and the tone button.
  static const double toneGap = S.s8;

  /// Space above a section title. Generous enough that the title reads as
  /// belonging to the run under it rather than the one above.
  static const double sectionTopGap = S.s16;
  static const double sectionBottomGap = S.s8;

  static const double flyoutItemGap = S.s4;

  static const double toneButtonRadius = CornerRadius.full;
  static const double flyoutRadius = CornerRadius.px32;

  /// Gap the flyout keeps above the point it opened from, so the finger that
  /// opened it doesn't cover the swatches.
  static const double flyoutAnchorGap = S.s24;

  /// Gap the flyout keeps from the viewport's safe edges.
  static const double flyoutEdgeInset = S.s8;

  static const MotionPreset flyoutEnter = MotionPreset.short;

  static const ReactionEmojiMenuTokens phone = ReactionEmojiMenuTokens(
    headerHeight: S.s40,
    headerGap: S.s12,
    cellExtent: S.s64,
    cellGap: S.s4,
    fadeHeight: S.s32,
    flyoutPadding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
    flyoutItemPadding: EdgeInsets.zero,
    flyoutHelpGap: S.s12,
  );

  /// Denser than [phone]: a pointer hits a smaller cell reliably, so the grid
  /// drops to a size that fits far more of the set in the same panel. The
  /// flyout goes the other way, since its swatches keep their glyph size and
  /// need the room around them to stay separable.
  static const ReactionEmojiMenuTokens desktop = ReactionEmojiMenuTokens(
    headerHeight: S.s32,
    headerGap: S.s16,
    cellExtent: S.s40,
    cellGap: S.s8,
    fadeHeight: S.s24,
    flyoutPadding: EdgeInsets.symmetric(horizontal: S.s24, vertical: S.s16),
    flyoutItemPadding: EdgeInsets.all(S.s4),
    flyoutHelpGap: S.s8,
  );

  static ReactionEmojiMenuTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
