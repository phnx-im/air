// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout, gesture, and motion tokens for the fullscreen picture takeover.
///
/// Geometry only: colors come from the dark palette at paint time, since the
/// takeover is dark in either theme.
@immutable
class FullscreenImageTokens {
  const FullscreenImageTokens({
    required this.headerPadding,
    required this.closeOnTrailingEdge,
    required this.shareInHeader,
    required this.frostedBackdrop,
    required this.dragToDismiss,
  });

  final EdgeInsets headerPadding;

  /// Which end of the header the close button takes. A pointer reaches for the
  /// far corner, a thumb for the near one.
  final bool closeOnTrailingEdge;

  /// Whether the header carries a share button opposite the close. A pointer
  /// already has the message it opened from within reach, so only the thumb
  /// gets one here.
  final bool shareInHeader;

  /// Whether the backdrop frosts what sits behind it instead of covering it.
  /// Only meaningful under a route that still paints the page below.
  final bool frostedBackdrop;

  /// Whether dragging the picture down dismisses the takeover. Touch only: a
  /// pointer has the close button and the escape key.
  final bool dragToDismiss;

  /// Inset between the viewport edge and the picture, so a full-bleed photo
  /// still reads as sitting on the backdrop.
  static const double imagePadding = S.s16;

  /// Shortest the header may be. It grows past this where its own padding asks
  /// for more, which is what turns the strip into a floating corner button.
  static const double headerHeight = Chrome.barHeight;

  static const double buttonSize = ButtonIconSize.s32;
  static const double buttonIconSize = S.s16;
  static const ButtonIconVariant buttonVariant = ButtonIconVariant.transparent;

  /// Inset from the viewport edge to a nav arrow. The picture is inset by less,
  /// so the arrows sit over it rather than beside it.
  static const double navEdgePadding = S.s16;

  /// Diameter of a nav arrow, and the chevron in it. Larger than the header
  /// glyph: we reach for the arrows while looking at the picture, not at them.
  static const double navButtonSize = ButtonIconSize.s32;
  static const double navIconSize = S.s20;

  /// Resting alpha of an arrow with somewhere left to go. Held off opaque so it
  /// reads as floating over the picture rather than printed on it.
  static const double navIdleOpacity = Alpha.a80;

  /// Fade of an arrow as it runs out of pictures to reach.
  static const MotionPreset navFadeMotion = MotionPreset.short;

  /// How far up from the bottom edge the gallery counter sits. The picture is
  /// inset to clear it, so this also sets part of that reserve.
  static const double counterBottom = S.s48;

  static const BlurLevel backdropBlur = BlurLevel.thick;

  /// Zoom ceiling, as a multiple of the scale that fills the viewport.
  static const double maxZoomScale = 4.0;

  /// Fraction of the current scale one scroll notch adds or removes.
  static const double scrollZoomStep = 0.12;

  /// Drag distance past which release dismisses rather than springs back.
  static const double dismissThreshold = 120;

  /// Distance over which the drag fades the backdrop out entirely.
  static const double dismissFadeDistance = 300;

  /// Distance over which the drag shrinks the picture to [dismissMinScale].
  static const double dismissScaleDistance = 600;
  static const double dismissMinScale = 0.3;

  /// Fade of the chrome as it comes and goes, and the glide from one picture to
  /// the next: a step through the gallery is the same gesture-free move.
  static const MotionPreset chromeMotion = MotionPreset.regular;

  /// How long a tap waits before it toggles the chrome. A second tap inside the
  /// window is a double tap meant for the zoom, so it cancels the toggle.
  static const Duration chromeTapDelay = Duration(milliseconds: 250);

  /// Broken-image glyph shown when the picture fails to decode.
  static const double errorIconSize = S.s48;

  static const FullscreenImageTokens phone = FullscreenImageTokens(
    headerPadding: EdgeInsets.symmetric(horizontal: S.s16),
    closeOnTrailingEdge: false,
    shareInHeader: true,
    // Opaque, so the takeover reads as a place of its own rather than a layer
    // over the conversation.
    frostedBackdrop: false,
    dragToDismiss: true,
  );

  /// The header floats in the corner rather than spanning a strip, and the
  /// backdrop frosts instead of covering, so the window underneath stays
  /// readable as context. Sharing stays with the message the picture came from.
  static const FullscreenImageTokens desktop = FullscreenImageTokens(
    headerPadding: EdgeInsets.all(S.s16),
    closeOnTrailingEdge: true,
    shareInHeader: false,
    frostedBackdrop: true,
    dragToDismiss: false,
  );

  /// The set for the current device. Keyed on the device rather than the
  /// viewport: the split is between what a thumb can do and what a pointer can,
  /// not how wide the window happens to be.
  static FullscreenImageTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
