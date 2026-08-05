// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Visual treatment of a [ButtonIcon]. The variant picks the fill, the blur,
/// and the shadow together, so a call site never assembles a look by hand.
enum ButtonIconVariant {
  /// Frosted material: a translucent tint over a backdrop blur, lifted by a
  /// knockout drop shadow. For chrome that floats over scrolling content.
  elevated,

  /// Opaque fill, flat by default. For chrome that sits on a surface rather
  /// than over it.
  solid,

  /// Translucent tint, no blur and no shadow. For a button inside an already
  /// elevated container.
  transparent,

  /// No fill at all: the glyph, plus the interaction states.
  plain,
}

/// Diameters on the DS scale, and the glyph each one pairs with.
///
/// A diameter is a plain double rather than a scale member: buttons sized off
/// type metrics (the composer, the message hover affordance) have to land
/// between two steps to line up with the text beside them.
abstract final class ButtonIconSize {
  static const double s24 = S.s24;
  static const double s32 = S.s32;
  static const double s40 = S.s40;
  static const double s48 = S.s48;
  static const double s64 = S.s64;

  /// Glyph diameter paired with a button [diameter]. An off-scale diameter
  /// takes the pair of the next step up, so a metric-derived button still gets
  /// a glyph from the scale.
  static double glyphFor(double diameter) => switch (diameter) {
    <= s24 => S.s12,
    <= s32 => S.s16,
    <= s40 => S.s20,
    <= s48 => S.s24,
    _ => S.s32,
  };
}

/// The look of each [ButtonIconVariant], resolved against the palette.
abstract final class ButtonIconTokens {
  /// Backdrop-blur radius behind [ButtonIconVariant.elevated] -- the frosted
  /// half of the treatment, the translucent [fill] being the other. A button
  /// is small enough that a heavier blur has too little area to resolve into
  /// anything but a flat wash.
  static final double elevatedBlur = Effect.blur(BlurLevel.thin);

  /// Alpha the glyph fades to without a handler. Content carries the disabled
  /// read, so it's what fades hardest.
  static const double disabledOpacity = Alpha.a40;

  /// Alpha the fill recedes to without a handler. It steps back rather than
  /// dissolving: a disabled control still has to read as a control, and the
  /// elevated fill is already translucent, so fading it in step with the glyph
  /// would leave nothing but a ghost.
  static const double disabledFillOpacity = Alpha.a80;

  static Color fill(SemanticPalette palette, ButtonIconVariant variant) =>
      switch (variant) {
        ButtonIconVariant.elevated => palette.backgroundMaterial.primary,
        ButtonIconVariant.solid => palette.backgroundElevated.secondary,
        ButtonIconVariant.transparent => palette.fill.tertiary,
        ButtonIconVariant.plain => const Color(0x00000000),
      };

  /// Drop shadow per variant. Solid is flat by default: it sits on a surface,
  /// so it has nothing to lift off. Callers that do want it lifted pass their
  /// own.
  static List<BoxShadow> shadows(ButtonIconVariant variant) =>
      switch (variant) {
        ButtonIconVariant.elevated => Effect.elevation(Elevation.small),
        ButtonIconVariant.solid ||
        ButtonIconVariant.transparent ||
        ButtonIconVariant.plain => const [],
      };
}
