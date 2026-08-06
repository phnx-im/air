// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry of the plate behind an [AppIconBadge]'s glyph.
///
/// Geometry only: colors come from the palette at paint time. Nothing varies
/// with density, since the host names the glyph size and the plate follows
/// from it.
abstract final class AppIconBadgeTokens {
  static const double radius = CornerRadius.px12;

  /// Inset around the glyph as a fraction of the glyph's own size, so the plate
  /// keeps its proportions at whatever size the host asks for. Half the glyph
  /// on each side puts the plate at twice the glyph.
  static const double paddingRatio = 0.5;

  static EdgeInsets padding(double glyphSize) =>
      EdgeInsets.all(glyphSize * paddingRatio);
}
