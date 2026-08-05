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
    required this.gap,
    required this.fieldPadding,
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
  final double gap;

  /// Horizontal inset between the field's edge and its content. The vertical
  /// inset comes from the type metrics, see [MessageInputTokens.fieldInsetY].
  final double fieldPadding;

  /// Unread dot pinned to the scroll-back button's top-right corner. It sits
  /// flush with the right edge, where the glyph leaves the most room.
  static const double dotSize = S.s8;
  static const double dotInsetTop = S.s2;
  static const double dotInsetRight = S.s0;

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
      textDirection: TextDirection.ltr,
    )..layout()).height;
    return ((buttonSize - line) / 2).clamp(0.0, double.infinity);
  }

  static const MessageInputTokens phone = MessageInputTokens(
    buttonSize: S.s40,
    inputRadius: CornerRadius.px20,
    containerPadding: EdgeInsets.only(left: S.s16, right: S.s16, bottom: S.s8),
    gap: S.s8,
    fieldPadding: S.s16,
  );

  // Horizontal inset matches the chat header's, so the attach button and the
  // header's back button line up on the same x instead of stepping inward down
  // the screen.
  static const MessageInputTokens desktop = MessageInputTokens(
    buttonSize: S.s32,
    inputRadius: CornerRadius.px16,
    containerPadding: EdgeInsets.only(left: S.s20, right: S.s20, bottom: S.s16),
    gap: S.s8,
    fieldPadding: S.s16,
  );

  static MessageInputTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
