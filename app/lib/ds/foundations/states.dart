// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/dimensions.dart';

/// Interaction-state tokens shared by every interactive surface, so hover,
/// press, and focus look the same wherever they appear. Applied through
/// `StateLayer`.
abstract final class StateTokens {
  /// Max alpha of the hover "lift forward" wash (white), at a fully light
  /// surface. It only tints the background, never the content, and is scaled by
  /// the surface luminance, so a light-grey surface lifts bright toward white
  /// while a dark or saturated one lifts only gently and never washes out.
  static const double hoverOverlay = Alpha.a60;

  /// Alpha of the hover wash where a surface is too light to brighten and it
  /// darkens instead (e.g. a white popover menu). Kept gentle.
  static const double hoverDarkenOverlay = Alpha.a5;

  /// Alpha of the press wash (recede).
  static const double pressedOverlay = Alpha.a20;

  /// Scale a surface dips to while pressed. Touch feedback.
  static const double pressedScale = 0.96;

  /// Scale a surface lifts to on pointer hover. The desktop mirror of
  /// [pressedScale].
  static const double hoverScale = 1.04;

  /// Stroke width of the keyboard focus ring, drawn in `function.link`.
  static const double focusRingWidth = StrokeWidth.px2;

  // Hover lifts a surface forward (white wash), press recedes it (black wash).
  // At the extremes that flips to whatever is visible, since you cannot
  // brighten white or darken black.

  /// At or above this surface luminance a white hover wash will not register,
  /// so hover darkens instead. Near-white surfaces only.
  static const double lightSurfaceCeiling = 0.95;

  /// At or below this surface luminance a black press wash will not register,
  /// so press lightens instead. Near-black surfaces only.
  static const double darkSurfaceFloor = 0.05;
}
