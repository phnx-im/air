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
    required this.imagePadding,
    required this.headerHeight,
    required this.headerPadding,
    required this.closeOnTrailingEdge,
    required this.shareInHeader,
    required this.buttonSize,
    required this.buttonIconSize,
    required this.buttonVariant,
    required this.navEdgePadding,
    required this.navButtonSize,
    required this.navIconSize,
    required this.navIdleOpacity,
    required this.navFadeMotion,
    required this.counterBottom,
    required this.frostedBackdrop,
    required this.backdropBlur,
    required this.maxZoomScale,
    required this.scrollZoomStep,
    required this.dragToDismiss,
    required this.dismissThreshold,
    required this.dismissFadeDistance,
    required this.dismissScaleDistance,
    required this.dismissMinScale,
    required this.chromeMotion,
    required this.chromeTapDelay,
    required this.errorIconSize,
  });

  /// Inset between the viewport edge and the picture, so a full-bleed photo
  /// still reads as sitting on the backdrop.
  final double imagePadding;

  /// Shortest the header may be. It grows past this where its own padding asks
  /// for more, which is what turns the strip into a floating corner button.
  final double headerHeight;
  final EdgeInsets headerPadding;

  /// Which end of the header the close button takes. A pointer reaches for the
  /// far corner, a thumb for the near one.
  final bool closeOnTrailingEdge;

  /// Whether the header carries a share button opposite the close. A pointer
  /// already has the message it opened from within reach, so only the thumb
  /// gets one here.
  final bool shareInHeader;

  final double buttonSize;
  final double buttonIconSize;
  final ButtonIconVariant buttonVariant;

  /// Inset from the viewport edge to a nav arrow. The picture is inset by less,
  /// so the arrows sit over it rather than beside it.
  final double navEdgePadding;

  /// Diameter of a nav arrow, and the chevron in it. Larger than the header
  /// glyph: we reach for the arrows while looking at the picture, not at them.
  final double navButtonSize;
  final double navIconSize;

  /// Resting alpha of an arrow with somewhere left to go. Held off opaque so it
  /// reads as floating over the picture rather than printed on it.
  final double navIdleOpacity;

  /// Fade of an arrow as it runs out of pictures to reach.
  final MotionPreset navFadeMotion;

  /// How far up from the bottom edge the gallery counter sits. The picture is
  /// inset to clear it, so this also sets part of that reserve.
  final double counterBottom;

  /// Whether the backdrop frosts what sits behind it instead of covering it.
  /// Only meaningful under a route that still paints the page below.
  final bool frostedBackdrop;
  final BlurLevel backdropBlur;

  /// Zoom ceiling, as a multiple of the scale that fills the viewport.
  final double maxZoomScale;

  /// Fraction of the current scale one scroll notch adds or removes.
  final double scrollZoomStep;

  /// Whether dragging the picture down dismisses the takeover. Touch only: a
  /// pointer has the close button and the escape key.
  final bool dragToDismiss;

  /// Drag distance past which release dismisses rather than springs back.
  final double dismissThreshold;

  /// Distance over which the drag fades the backdrop out entirely.
  final double dismissFadeDistance;

  /// Distance over which the drag shrinks the picture to [dismissMinScale].
  final double dismissScaleDistance;
  final double dismissMinScale;

  /// Fade of the chrome as it comes and goes, and the glide from one picture to
  /// the next: a step through the gallery is the same gesture-free move.
  final MotionPreset chromeMotion;

  /// How long a tap waits before it toggles the chrome. A second tap inside the
  /// window is a double tap meant for the zoom, so it cancels the toggle.
  final Duration chromeTapDelay;

  /// Broken-image glyph shown when the picture fails to decode.
  final double errorIconSize;

  static const FullscreenImageTokens phone = FullscreenImageTokens(
    imagePadding: S.s16,
    headerHeight: S.s56,
    headerPadding: EdgeInsets.symmetric(horizontal: S.s16),
    closeOnTrailingEdge: false,
    shareInHeader: true,
    buttonSize: ButtonIconSize.s32,
    buttonIconSize: S.s16,
    buttonVariant: ButtonIconVariant.transparent,
    navEdgePadding: S.s16,
    navButtonSize: ButtonIconSize.s32,
    navIconSize: S.s20,
    navIdleOpacity: Alpha.a80,
    navFadeMotion: MotionPreset.short,
    counterBottom: S.s48,
    // Opaque, so the takeover reads as a place of its own rather than a layer
    // over the conversation.
    frostedBackdrop: false,
    backdropBlur: BlurLevel.thick,
    maxZoomScale: 4.0,
    scrollZoomStep: 0.12,
    dragToDismiss: true,
    dismissThreshold: 120,
    dismissFadeDistance: 300,
    dismissScaleDistance: 600,
    dismissMinScale: 0.3,
    chromeMotion: MotionPreset.regular,
    chromeTapDelay: Duration(milliseconds: 250),
    errorIconSize: S.s48,
  );

  /// The header floats in the corner rather than spanning a strip, and the
  /// backdrop frosts instead of covering, so the window underneath stays
  /// readable as context. Sharing stays with the message the picture came from.
  static const FullscreenImageTokens desktop = FullscreenImageTokens(
    imagePadding: S.s16,
    headerHeight: S.s56,
    headerPadding: EdgeInsets.all(S.s16),
    closeOnTrailingEdge: true,
    shareInHeader: false,
    buttonSize: ButtonIconSize.s32,
    buttonIconSize: S.s16,
    buttonVariant: ButtonIconVariant.transparent,
    navEdgePadding: S.s16,
    navButtonSize: ButtonIconSize.s32,
    navIconSize: S.s20,
    navIdleOpacity: Alpha.a80,
    navFadeMotion: MotionPreset.short,
    counterBottom: S.s48,
    frostedBackdrop: true,
    backdropBlur: BlurLevel.thick,
    maxZoomScale: 4.0,
    scrollZoomStep: 0.12,
    dragToDismiss: false,
    dismissThreshold: 120,
    dismissFadeDistance: 300,
    dismissScaleDistance: 600,
    dismissMinScale: 0.3,
    chromeMotion: MotionPreset.regular,
    chromeTapDelay: Duration(milliseconds: 250),
    errorIconSize: S.s48,
  );

  /// The set for the current device. Keyed on the device rather than the
  /// viewport: the split is between what a thumb can do and what a pointer can,
  /// not how wide the window happens to be.
  static FullscreenImageTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
