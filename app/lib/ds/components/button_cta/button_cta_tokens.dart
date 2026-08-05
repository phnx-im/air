// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Visual treatment of a [ButtonCTA]. The type picks the fill and the glyph
/// color together, so a call site never assembles a look by hand.
enum ButtonCTAType {
  /// Brand accent fill. For the one action a surface leads with.
  primary,

  /// Tinted fill. For the actions standing next to the primary one.
  secondary,
}

/// Layout tokens for a call-to-action button, per density.
///
/// Geometry only: colors come from the palette at paint time, picked by
/// [ButtonCTAType].
@immutable
class ButtonCTATokens {
  const ButtonCTATokens({required this.size, required this.iconSize});

  /// Diameter of the circle.
  final double size;

  final double iconSize;

  /// Gap between the circle and the label under it.
  static const double labelGap = S.s8;

  static const ButtonCTATokens phone = ButtonCTATokens(
    size: S.s64,
    iconSize: S.s32,
  );

  static const ButtonCTATokens desktop = ButtonCTATokens(
    size: S.s48,
    iconSize: S.s24,
  );

  static ButtonCTATokens get current => DeviceType.isPhone ? phone : desktop;
}
