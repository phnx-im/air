// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the mention picker, per density: the floating card and
/// the member rows inside it.
///
/// Geometry plus the picker's color roles as palette resolvers. The picker has
/// no variants. Its roles are fixed and belong here rather than at the call
/// site.
@immutable
class MentionPickerTokens {
  const MentionPickerTokens({
    required this.rowPadding,
    required this.avatarSize,
  });

  final EdgeInsets rowPadding;
  final double avatarSize;

  /// Fixed rather than content-sized. The card holds still while typing narrows
  /// the list, and longer names are ellipsized.
  static const double width = S.s240;

  static const double radius = CornerRadius.px16;
  static const Elevation elevation = Elevation.large;

  /// Inset between the card's edge and its rows. A highlighted row then reads
  /// as a rounded chip inside the card rather than a stripe across it.
  static const EdgeInsets padding = EdgeInsets.all(S.s4);

  static const double rowGap = S.s2;
  static const double rowRadius = CornerRadius.px12;
  static const double avatarGap = S.s8;

  static Color background(SemanticPalette palette) =>
      palette.backgroundElevated.primary;

  /// Behind the highlighted row, the one Enter would pick.
  static Color highlightFill(SemanticPalette palette) => palette.fill.tertiary;

  static const MentionPickerTokens phone = MentionPickerTokens(
    rowPadding: EdgeInsets.all(S.s8),
    avatarSize: S.s32,
  );

  static const MentionPickerTokens desktop = MentionPickerTokens(
    rowPadding: EdgeInsets.symmetric(horizontal: S.s8, vertical: S.s4),
    avatarSize: S.s24,
  );

  static MentionPickerTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
