// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/reaction_chip/reaction_chip_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the run of reaction chips under a message, per density.
///
/// Geometry only: colors come from the palette at paint time.
@immutable
class ReactionStripTokens {
  const ReactionStripTokens({required this.spacing, required this.overlap});

  /// Gap between adjacent chips.
  final double spacing;

  /// How far the pill rides up over the bubble's bottom edge, so the reactions
  /// read as cropped into the message rather than as a row of their own. The
  /// crop ring sits outside the pill, so the chip box bites this plus
  /// [ReactionChipTokens.cropWidth] into the bubble.
  final double overlap;

  /// How far the chip box rides up over the bubble's bottom edge: the pill's
  /// [overlap] plus the ring cropped around it. A host that reserves the run's
  /// height below the bubble gets this much of it back, since the run stops
  /// short of the foot of what it reserved.
  double get lift => overlap + ReactionChipTokens.cropWidth;

  /// Inset from the message's leading edge to the first chip, so the run starts
  /// just inside the bubble's start.
  static const double startInset = S.s8;

  /// Rounding slack on the fit test. We measure the chip widths, the strip's
  /// own width comes from layout, and a sub-pixel disagreement between the two
  /// shouldn't cost a chip.
  static const double fitSlack = 1.0;

  static const ReactionStripTokens phone = ReactionStripTokens(
    spacing: S.s4,
    overlap: S.s2,
  );

  /// A denser run that bites a little deeper, matching the smaller chips the
  /// content pane renders.
  static const ReactionStripTokens desktop = ReactionStripTokens(
    spacing: S.s2,
    overlap: S.s4,
  );

  static ReactionStripTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
