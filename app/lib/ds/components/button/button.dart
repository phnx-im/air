// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart';

enum AppButtonSize { small, large }

enum AppButtonType { primary, secondary }

enum AppButtonTone { normal, danger }

enum AppButtonState { active, inactive, pending }

class AppButton extends StatelessWidget {
  const AppButton({
    super.key,
    this.size = AppButtonSize.large,
    this.type = AppButtonType.primary,
    this.tone = AppButtonTone.normal,
    this.state = AppButtonState.active,
    required this.onPressed,
    this.onLongPress,
    this.icon,
    required this.label,
    this.alignment = MainAxisAlignment.center,
  });

  final AppButtonSize size;
  final AppButtonType type;
  final AppButtonTone tone;
  final AppButtonState state;

  final VoidCallback onPressed;
  final VoidCallback? onLongPress;

  final Function(Size size, Color color)? icon;
  final String label;
  final MainAxisAlignment alignment;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    final foregroundColor = switch ((type, state, tone)) {
      (.primary, .inactive, .danger) =>
        palette.function.neutral.white.withValues(alpha: Alpha.a50),
      (.primary, .inactive, .normal) =>
        palette.function.neutral.toggleWhite.withValues(alpha: Alpha.a50),
      (.primary, _, .danger) => palette.function.neutral.white,
      (.primary, _, .normal) => palette.function.neutral.toggleWhite,
      (.secondary, .inactive, .danger) => palette.function.danger.withValues(
        alpha: Alpha.a50,
      ),
      (.secondary, .inactive, _) =>
        palette.function.neutral.toggleBlack.withValues(alpha: Alpha.a50),
      (.secondary, _, .danger) => palette.function.danger,
      (.secondary, _, _) => palette.function.neutral.toggleBlack,
    };

    final backgroundColor = switch ((type, tone)) {
      (.primary, .danger) => palette.function.danger,
      (.primary, .normal) => palette.accentBrand.primary,
      (.secondary, _) => palette.accentBrand.tertiary,
    };

    const Border? border = null;

    final iconColor = switch ((type, state)) {
      (.secondary, _) => palette.text.primary,
      _ => foregroundColor,
    };

    final verticalPadding = switch (size) {
      AppButtonSize.small => S.s8,
      AppButtonSize.large => S.s12,
    };

    final iconSize = switch (size) {
      AppButtonSize.small => const Size.square(S.s16),
      AppButtonSize.large => const Size.square(S.s24),
    };

    final labelToken = switch (size) {
      AppButtonSize.small => typeScale.body.xs,
      AppButtonSize.large => typeScale.body.regular,
    };

    final borderRadius = switch (size) {
      AppButtonSize.small => CornerRadius.px8,
      AppButtonSize.large => CornerRadius.px12,
    };

    return OutlinedButton(
      onPressed: state == .active ? onPressed : null,
      onLongPress: state == .active ? onLongPress : null,
      style: ButtonStyle(
        visualDensity: .compact,
        padding: const WidgetStatePropertyAll(EdgeInsets.zero),
        backgroundColor: WidgetStatePropertyAll(backgroundColor),
        overlayColor: WidgetStatePropertyAll(backgroundColor),
        shape: WidgetStatePropertyAll(
          RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(borderRadius),
            side: border != null
                ? BorderSide(color: border.top.color)
                : BorderSide.none,
          ),
        ),
        side: border != null
            ? WidgetStatePropertyAll(BorderSide(color: border.top.color))
            : null,
      ),
      child: Padding(
        padding: EdgeInsets.symmetric(
          vertical: verticalPadding,
          horizontal: 12,
        ),
        child: Row(
          mainAxisAlignment: alignment,
          crossAxisAlignment: .center,
          children: [
            if (state == .pending)
              SizedBox(
                width: iconSize.width,
                height: iconSize.height,
                child: CircularProgressIndicator(
                  color: foregroundColor,
                  strokeWidth: StrokeWidth.px2,
                ),
              ),

            if (state != .pending && icon != null) ...[
              icon?.call(iconSize, iconColor),
              const SizedBox(width: 8),
            ],

            if (state != .pending)
              SizedBox(
                height: iconSize.height,
                child: Center(
                  child: Text(
                    label,
                    style: labelToken.style(color: foregroundColor),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
