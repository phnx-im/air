// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the message input, per density.
///
/// Geometry only: colors come from the palette at paint time, and the host
/// supplies the surface the input floats over.
@immutable
class MessageInputTokens {
  const MessageInputTokens({
    required this.buttonSize,
    required this.inputRadius,
    required this.containerPadding,
  });

  /// Diameter of the round buttons, and the floor for the field height, so an
  /// empty input reads as one row of equal-height elements.
  final double buttonSize;

  /// The field's corner radius. Fixed, not a pill: at [buttonSize] an empty
  /// field is already a pill, and once it grows to several lines we want it to
  /// keep these corners instead of stretching into a stadium.
  final double inputRadius;

  /// Inset between the input and the edges of its region. No top inset, so the
  /// conversation scrolls right up to the input.
  final EdgeInsets containerPadding;

  /// Gap between the buttons and the field.
  static const double gap = Chrome.controlGap;

  /// Horizontal inset between the field's edge and its content. The vertical
  /// inset comes from the type metrics, see [MessageInputTokens.fieldInsetY].
  static const double fieldPadding = S.s16;

  /// Scale the trailing buttons grow from as they fade in. Send pops harder
  /// than scroll-back, since it answers something the user just typed while
  /// scroll-back only reports where the reader is.
  static const double sendEnterScale = 0.8;
  static const double scrollBackEnterScale = 0.9;

  /// Send arrives on the shortest preset, so the button is under the thumb by
  /// the time the first character lands. Scroll-back reveals and dismisses on
  /// the regular preset, the same transition both ways.
  static const MotionPreset sendMotion = MotionPreset.short;
  static const MotionPreset scrollBackMotion = MotionPreset.regular;

  /// Symmetric vertical inset that centers a single line in the
  /// [buttonSize]-tall field. Since it's symmetric and the row is
  /// bottom-anchored, the last line stays a fixed distance from the bottom edge
  /// and never shifts as the field grows.
  ///
  /// We measure it instead of reading [TypeStyleToken.lineHeightPx], which
  /// rounds the ratio and lands a fraction of a pixel off the font's own line
  /// box. An empty field has to match [buttonSize] exactly, or it sits proud of
  /// the buttons beside it.
  double get fieldInsetY {
    final line = (TextPainter(
      text: TextSpan(text: 'Ag', style: typeScale.body.regular.style()),
      textDirection: .ltr,
    )..layout()).height;
    return ((buttonSize - line) / 2).clamp(0.0, double.infinity);
  }

  static const MessageInputTokens phone = MessageInputTokens(
    buttonSize: S.s40,
    inputRadius: CornerRadius.px20,
    containerPadding: EdgeInsets.only(left: S.s16, right: S.s16, bottom: S.s8),
  );

  static const MessageInputTokens desktop = MessageInputTokens(
    buttonSize: S.s32,
    inputRadius: CornerRadius.px16,
    containerPadding: EdgeInsets.only(
      left: Chrome.edgeInset,
      right: Chrome.edgeInset,
      bottom: S.s16,
    ),
  );

  /// The set for the current device. Keyed on the device rather than the
  /// viewport: the buttons are touch targets, so they size to what taps them
  /// and not to how wide the window happens to be.
  static MessageInputTokens get current => DeviceType.isPhone ? phone : desktop;
}

/// Layout tokens for the staged-reply quote shown above the field.
abstract final class MessageInputQuoteTokens {
  /// Gap between the field chrome's top edge and the staged quote. The chrome
  /// has no top inset of its own, so the block carries it.
  static const double gapAbove = S.s8;

  /// Gap between the quote and the text being written. It sits outside the
  /// fill, so the two read as separate things sharing the chrome rather than
  /// one box crowding the other. The field drops its own top inset while
  /// anything sits above it, so this is the whole of that gap.
  static const double gapBelow = S.s8;

  /// Inset from the chrome's side edges. Narrower than the field's own, so the
  /// quote reads as a block set into the chrome rather than another line of
  /// text sharing the composer's left edge.
  static const double gapSide = S.s8;

  /// Inset between the fill and the quoted text, even all round: the quote is
  /// two short lines, and [gapBelow] keeps the field clear of the fill.
  static const EdgeInsets padding = EdgeInsets.symmetric(
    horizontal: S.s12,
    vertical: S.s8,
  );

  static const double radius = CornerRadius.px12;

  /// The rule down the leading edge, which is what marks the text as quoted.
  static const double accentWidth = StrokeWidth.px1;

  /// Gap between the accent rule and the quoted text.
  static const double accentGap = S.s12;

  /// Gap between the sender's name and the preview under it.
  static const double senderGap = S.s2;

  /// Gap between the quoted text and the still of a quoted picture.
  static const double thumbGap = S.s8;

  /// Lines of the quoted message shown before it ellipsizes.
  static const int previewMaxLines = 2;

  /// The dismiss button: a small circle with a generous ring of hit target
  /// around it.
  static const double removeSize = S.s20;
  static const double removeHitTarget = S.s32;
  static const double removeIconSize = S.s12;

  /// How far the dismiss circle hangs past the block's top-right corner, so it
  /// reads as pinned to the corner rather than sitting inside the quote.
  ///
  /// Capped by the chrome's own rounded corner, which the circle has to stay
  /// within. At [gapSide] and [gapAbove] the phone's radius clears an overhang
  /// of about 5, so this keeps a pixel in hand.
  static const double removeOverhang = S.s4;

  /// Offset for the button, which lays out at [removeHitTarget] with the circle
  /// centred in it, so the ring has to be backed out for [removeOverhang] to
  /// measure from the circle's own edge.
  static const double removeOffset =
      -(removeOverhang + (removeHitTarget - removeSize) / 2);
}
