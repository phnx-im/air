// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a reaction chip, per density.
///
/// Geometry only: colors come from the palette at paint time, and the
/// surface the chip is cropped out of is the host's to supply.
@immutable
class ReactionChipTokens {
  const ReactionChipTokens({required this.padding, required this.minHeight});

  /// Inset between the pill's edge and its content. The glyph is pinned to a
  /// 100% line, so the vertical inset is what keeps the emoji off the pill's
  /// edge once a scaled-up count grows the pill past [minHeight].
  final EdgeInsets padding;

  /// Floor for the pill's height, not a fixed one: a scaled-up count grows the
  /// pill rather than getting clipped by it.
  final double minHeight;

  /// Gap between the glyph and its count.
  static const double countGap = S.s4;

  /// Both the pill and the ring around it are stadiums, so one sentinel covers
  /// the two of them.
  static const double radius = CornerRadius.full;

  /// Ring painted around the pill in the color behind the message, so the chip
  /// reads as cropped out of the bubble it overlaps instead of stacked on it.
  /// Pure padding around the pill, so it widens the chip on every side and the
  /// pill itself keeps its full [minHeight].
  static const double cropWidth = StrokeWidth.px1_5;

  static const ReactionChipTokens phone = ReactionChipTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s8, vertical: S.s4),
    minHeight: S.s28,
  );

  /// Denser than [phone]: the chips sit under a bubble in the narrower content
  /// pane, where the type scale is a step smaller too.
  static const ReactionChipTokens desktop = ReactionChipTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s8, vertical: S.s2),
    minHeight: S.s24,
  );

  static ReactionChipTokens get current => DeviceType.isPhone ? phone : desktop;
}
