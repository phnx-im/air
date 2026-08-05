// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';

/// Layout tokens for a picture in a message bubble.
///
/// Geometry only: colors come from the palette at paint time.
///
/// One set for both densities: the picture and the window it sits in size the
/// bubble, neither of which follows pointer density.
abstract final class MediaMessageTokens {
  static const double radius = CornerRadius.px12;

  /// Both natural dimensions under this and the picture reads as a thumbnail:
  /// it keeps its own size inside a fixed square frame instead of growing to
  /// fill a bubble.
  static const double thumbnailMin = S.s96;

  /// Even inset between the thumbnail frame and the picture inside it.
  static const double thumbnailPadding = S.s8;

  /// Ceiling on the rendered height. A taller picture scales down until it
  /// fits, so one photo can never take the whole conversation window.
  static const double maxHeight = 400;

  /// Floor on the rendered width. A picture so tall that fitting it to
  /// [maxHeight] would leave it narrower than this is cropped to cover a
  /// [minScaleWidth] by [maxHeight] frame rather than shrinking to a sliver.
  static const double minScaleWidth = 200;

  /// Broken-image glyph shown when the picture fails to decode.
  static const double errorIconSize = S.s24;
}
