// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a picture in a message bubble.
///
/// Geometry only: colors come from the palette at paint time.
@immutable
class MediaMessageTokens {
  const MediaMessageTokens({
    required this.radius,
    required this.thumbnailMin,
    required this.thumbnailPadding,
    required this.maxHeight,
    required this.minScaleWidth,
    required this.errorIconSize,
  });

  final double radius;

  /// Both natural dimensions under this and the picture reads as a thumbnail:
  /// it keeps its own size inside a fixed square frame instead of growing to
  /// fill a bubble.
  final double thumbnailMin;

  /// Even inset between the thumbnail frame and the picture inside it.
  final double thumbnailPadding;

  /// Ceiling on the rendered height. A taller picture scales down until it
  /// fits, so one photo can never take the whole conversation window.
  final double maxHeight;

  /// Floor on the rendered width. A picture so tall that fitting it to
  /// [maxHeight] would leave it narrower than this is cropped to cover a
  /// [minScaleWidth] by [maxHeight] frame rather than shrinking to a sliver.
  final double minScaleWidth;

  /// Broken-image glyph shown when the picture fails to decode.
  final double errorIconSize;

  /// One set for both densities: the picture and the window it sits in size the
  /// bubble, neither of which follows pointer density.
  static const MediaMessageTokens standard = MediaMessageTokens(
    radius: CornerRadius.px12,
    thumbnailMin: S.s96,
    thumbnailPadding: S.s8,
    maxHeight: 400,
    minScaleWidth: 200,
    errorIconSize: S.s24,
  );
}
