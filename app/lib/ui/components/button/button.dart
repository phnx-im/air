// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';

enum AppButtonSize { small, large }

enum AppButtonType { primary, secondary }

enum AppButtonState { active, inactive, danger, pending }

class AppButton extends StatelessWidget {
  const AppButton({
    super.key,
    this.size = AppButtonSize.large,
    this.type = AppButtonType.primary,
    this.state = AppButtonState.active,
    required this.onPressed,
    this.icon,
    required this.label,
  });

  final AppButtonSize size;
  final AppButtonType type;
  final AppButtonState state;

  final VoidCallback onPressed;

  final Function(Size size, Color color)? icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    final foregroundColor = switch ((type, state)) {
      (.primary, .active || .pending) => colors.function.toggleWhite,
      (.primary, .inactive) => colors.function.toggleWhite.withValues(
        alpha: 0.5,
      ),
      (.primary, .danger) => colors.function.white,
      (.secondary, _) => colors.function.toggleBlack,
    };

    final backgroundColor = switch ((type, state)) {
      (.primary, .active || .inactive || .pending) => colors.accent.primary,
      (.primary, .danger) => colors.function.danger,
      (.secondary, _) => colors.accent.quaternary,
    };

    final iconColor = switch ((type, state)) {
      (.secondary, _) => colors.text.primary,
      _ => foregroundColor,
    };

    final verticalPadding = switch (size) {
      AppButtonSize.small => Spacings.xxs,
      AppButtonSize.large => Spacings.xs,
    };

    final iconSize = switch (size) {
      AppButtonSize.small => const Size.square(Spacings.s),
      AppButtonSize.large => const Size.square(Spacings.m),
    };

    final labelSize = switch (size) {
      AppButtonSize.small => LabelFontSize.small2.size,
      AppButtonSize.large => LabelFontSize.base.size,
    };

    final borderRadius = switch (size) {
      AppButtonSize.small => Spacings.xxs,
      AppButtonSize.large => Spacings.xs,
    };

    return OutlinedButton(
      onPressed: state == AppButtonState.inactive ? null : () => onPressed(),
      style: ButtonStyle(
        visualDensity: .compact,
        padding: const WidgetStatePropertyAll(EdgeInsets.zero),
        backgroundColor: WidgetStatePropertyAll(backgroundColor),
        overlayColor: WidgetStatePropertyAll(backgroundColor),
        shape: WidgetStatePropertyAll(
          RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(borderRadius),
          ),
        ),
      ),
      child: Padding(
        padding: EdgeInsets.symmetric(
          vertical: verticalPadding,
          horizontal: 12,
        ),
        child: Row(
          mainAxisAlignment: .center,
          crossAxisAlignment: .center,
          children: [
            if (state == .pending)
              SizedBox(
                width: iconSize.width,
                height: iconSize.height,
                child: CircularProgressIndicator(
                  color: foregroundColor,
                  strokeWidth: 2,
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
                    style: TextStyle(
                      color: foregroundColor,
                      fontSize: labelSize,
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
