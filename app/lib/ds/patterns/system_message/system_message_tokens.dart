// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a system message, per density.
///
/// Geometry only: colors come from the palette at paint time. One bundle
/// covers both variants, so a host that renders a mix builds it once.
@immutable
class SystemMessageTokens {
  const SystemMessageTokens({
    required this.padding,
    required this.ruleWidth,
    required this.ruleThickness,
    required this.ruleGap,
    required this.timestampGap,
    required this.cardPadding,
    required this.cardRadius,
    required this.cardTitleGap,
    required this.cardTimestampGap,
    required this.cardMaxWidth,
  });

  /// Inset around the whole message, both variants.
  final EdgeInsets padding;

  /// The short rule above a notice. A fixed width rather than a share of the
  /// conversation: it's a mark, not a divider, and centering it is the point.
  final double ruleWidth;

  final double ruleThickness;

  /// Gap between the rule and the text under it.
  final double ruleGap;

  final double timestampGap;

  final EdgeInsets cardPadding;
  final double cardRadius;

  /// Gap between a card's title and its body.
  final double cardTitleGap;

  /// Wider than [timestampGap]: the card is a block, so its timestamp needs
  /// clear air to read as a caption under it rather than as part of it.
  final double cardTimestampGap;

  /// Cap on a card's width. A centered card wider than this stops reading as
  /// an announcement and starts reading as a wall of the conversation.
  final double cardMaxWidth;

  static const SystemMessageTokens phone = SystemMessageTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s16),
    ruleWidth: S.s128,
    ruleThickness: StrokeWidth.px1,
    ruleGap: S.s16,
    timestampGap: S.s8,
    cardPadding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s24),
    cardRadius: CornerRadius.px12,
    cardTitleGap: S.s16,
    cardTimestampGap: S.s16,
    cardMaxWidth: 400,
  );

  /// Denser than [phone]: the two-pane layout fits fewer messages in the same
  /// column, so a notice between them takes less of it.
  static const SystemMessageTokens desktop = SystemMessageTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s20, vertical: S.s12),
    ruleWidth: S.s128,
    ruleThickness: StrokeWidth.px1,
    ruleGap: S.s16,
    timestampGap: S.s8,
    cardPadding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s24),
    cardRadius: CornerRadius.px12,
    cardTitleGap: S.s16,
    cardTimestampGap: S.s16,
    cardMaxWidth: 400,
  );

  static SystemMessageTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
