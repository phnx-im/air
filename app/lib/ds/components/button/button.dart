// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/components/button/button_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart' show CircularProgressIndicator;
import 'package:flutter/widgets.dart';

export 'package:air/ds/components/button/button_tokens.dart';

/// Pill button with a label and an optional leading glyph. Its look follows
/// [type] / [tone] / [state] and its geometry follows [size]. Hover, press,
/// and focus ride on the shared [StateLayer], shaped by the platform -- touch
/// dips, a pointer lifts.
///
/// The pill takes the width it's given, with [alignment] placing the content
/// inside it.
class Button extends StatelessWidget {
  const Button({
    super.key,
    this.size = ButtonSize.large,
    this.type = ButtonType.primary,
    this.tone = ButtonTone.normal,
    this.state = ButtonState.active,
    required this.onPressed,
    this.onLongPress,
    this.icon,
    required this.label,
    this.alignment = MainAxisAlignment.center,
  });

  final ButtonSize size;
  final ButtonType type;
  final ButtonTone tone;
  final ButtonState state;

  final VoidCallback onPressed;
  final VoidCallback? onLongPress;

  /// Builds the leading glyph at the footprint and color the button picks for
  /// it. Absent in [ButtonState.pending], where the spinner takes the slot.
  final Function(Size size, Color color)? icon;
  final String label;
  final MainAxisAlignment alignment;

  @override
  Widget build(BuildContext context) {
    final tokens = ButtonTokens.of(size);
    final colors = _colors(SemanticPalette.of(context));
    final active = state == ButtonState.active;
    final phone = DeviceType.isPhone;

    final Widget pill = StateLayer(
      borderRadius: tokens.radius,
      surface: colors.fill,
      enabled: active,
      onTap: onPressed,
      onLongPress: onLongPress,
      hoverScale: true,
      background: DecoratedBox(
        decoration: BoxDecoration(
          color: colors.fill,
          borderRadius: BorderRadius.circular(tokens.radius),
        ),
      ),
      child: Container(
        height: tokens.height,
        padding: tokens.padding,
        child: Row(
          mainAxisAlignment: alignment,
          children: [
            if (state == ButtonState.pending)
              SizedBox.square(
                dimension: tokens.iconSize,
                child: CircularProgressIndicator(
                  color: colors.label,
                  strokeWidth: ButtonTokens.spinnerWidth,
                ),
              )
            else ...[
              if (icon != null) ...[
                icon!(Size.square(tokens.iconSize), colors.glyph),
                const SizedBox(width: ButtonTokens.iconLabelGap),
              ],
              Text(
                label,
                style: size.labelToken.style(color: colors.label, tight: true),
              ),
            ],
          ],
        ),
      ),
    );

    if (!phone || tokens.height >= ButtonTokens.minTouchHeight) {
      return pill;
    }

    // The pill is shorter than a finger needs, so a transparent ring around it
    // carries the tap too. The pill keeps the interaction states, and nested
    // detectors resolve innermost-first, so a tap on it fires the handler once.
    //
    // Padding rather than a sized box: it grows the footprint without touching
    // the width the pill would otherwise take, so a button in an unbounded row
    // still sizes to its label.
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: active ? onPressed : null,
      onLongPress: active ? onLongPress : null,
      child: Padding(
        padding: EdgeInsets.symmetric(
          vertical: (ButtonTokens.minTouchHeight - tokens.height) / 2,
        ),
        child: pill,
      ),
    );
  }

  /// The three colors the button paints with. A disabled button fades its
  /// content and keeps its fill, so the pill stays a pill.
  ({Color fill, Color label, Color glyph}) _colors(SemanticPalette palette) {
    final fill = switch ((type, tone)) {
      (.primary, .danger) => palette.function.danger,
      (.primary, .normal) => palette.accentBrand.primary,
      (.secondary, _) => palette.accentBrand.tertiary,
    };

    final label = switch ((type, tone)) {
      (.primary, .danger) => palette.function.neutral.white,
      (.primary, .normal) => palette.function.neutral.toggleWhite,
      (.secondary, .danger) => palette.function.danger,
      (.secondary, .normal) => palette.function.neutral.toggleBlack,
    };

    // A secondary button carries a tinted fill rather than a solid one, so its
    // glyph takes the text color instead of the label's tone color.
    final glyph = switch (type) {
      ButtonType.secondary => palette.text.primary,
      ButtonType.primary => label,
    };

    final fade = state == ButtonState.disabled
        ? StateTokens.disabledContent
        : Alpha.a100;

    return (
      fill: fill,
      label: label.withValues(alpha: label.a * fade),
      glyph: glyph.withValues(alpha: glyph.a * fade),
    );
  }
}
