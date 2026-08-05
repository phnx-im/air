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
  const SystemMessageTokens({required this.padding});

  /// Inset around the whole message, both variants.
  final EdgeInsets padding;

  /// The short rule above a notice. A fixed width rather than a share of the
  /// conversation: it's a mark, not a divider, and centering it is the point.
  static const double ruleWidth = S.s128;

  static const double ruleThickness = StrokeWidth.px1;

  /// Gap between the rule and the text under it.
  static const double ruleGap = S.s16;

  static const double timestampGap = S.s8;

  static const EdgeInsets cardPadding = EdgeInsets.symmetric(
    horizontal: S.s16,
    vertical: S.s24,
  );
  static const double cardRadius = CornerRadius.px12;

  /// Gap between a card's title and its body.
  static const double cardTitleGap = S.s16;

  /// Wider than [timestampGap]: the card is a block, so its timestamp needs
  /// clear air to read as a caption under it rather than as part of it.
  static const double cardTimestampGap = S.s16;

  /// Cap on a card's width. A centered card wider than this stops reading as
  /// an announcement and starts reading as a wall of the conversation.
  static const double cardMaxWidth = 400;

  static const SystemMessageTokens phone = SystemMessageTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s16),
  );

  /// Denser than [phone]: the two-pane layout fits fewer messages in the same
  /// column, so a notice between them takes less of it.
  static const SystemMessageTokens desktop = SystemMessageTokens(
    padding: EdgeInsets.symmetric(horizontal: S.s20, vertical: S.s12),
  );

  static SystemMessageTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
