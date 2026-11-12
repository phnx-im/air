// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a link preview card.
///
/// Geometry only: colors come from the palette at paint time. One set for
/// both densities.
abstract final class LinkPreviewTokens {
  /// Corner radius of the standalone card, matching a bubble's.
  static const double radius = CornerRadius.px12;

  /// Inset between the card's edge and its text block.
  static const EdgeInsets padding = EdgeInsets.all(S.s12);

  /// Banner aspect ratio, the Open Graph recommended 1.91:1.
  static const double bannerAspect = 1.91;

  /// Ceiling on the banner height. A wider crop beats a picture that
  /// swallows the conversation.
  static const double bannerMaxHeight = S.s192;

  /// Images narrower than this take the thumbnail shape.
  static const double bannerMinWidth = 300;

  /// Aspect ratios outside this range take the thumbnail shape.
  static const double bannerMinAspect = 1.25;
  static const double bannerMaxAspect = 4;

  static const double thumbSize = S.s64;
  static const double thumbRadius = CornerRadius.px8;

  /// Gap between the text block and the trailing thumbnail.
  static const double thumbGap = S.s16;

  /// Gap between the title and the description.
  static const double textGap = S.s2;

  /// Gap above the source line.
  static const double sourceGap = S.s4;

  static const double iconSize = S.s12;

  /// Gap between the globe and the address in the source line.
  static const double iconGap = S.s4;

  static const int titleMaxLines = 2;
  static const int descriptionMaxLines = 2;

  /// Inset of the pending card, which is a single line.
  static const EdgeInsets pendingPadding = EdgeInsets.symmetric(
    horizontal: S.s12,
    vertical: S.s8,
  );

  /// Gap between the ring and the address on the pending card.
  static const double pendingGap = S.s8;

  static const double pendingRingSize = S.s16;
  static const double pendingRingStroke = StrokeWidth.px2;

  /// How long the pending ring takes to fill.
  static const Duration pendingDuration = Duration(milliseconds: 1200);

  /// Hairline between a nested card and the message text below it, painted
  /// in the secondary separator color (`LinkPreview.dividerColorOf`).
  static const double dividerWidth = StrokeWidth.px0_5;

  /// Play badge over a video or music image: 56 across on a banner, 32 on a
  /// thumbnail.
  static const double badgePadding = S.s16;
  static const double badgeIconSize = S.s24;
  static const double badgePaddingCompact = S.s8;
  static const double badgeIconSizeCompact = S.s16;
  static final double badgeBlur = Effect.blur(BlurLevel.medium);
}
