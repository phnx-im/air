// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/spacings.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/button/glass_circle_button.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:flutter/material.dart';

class AppBarXButton extends StatelessWidget {
  const AppBarXButton({
    super.key,
    this.onPressed,
    this.backgroundColor,
    this.foregroundColor,
  });

  final VoidCallback? onPressed;
  final Color? backgroundColor;
  final Color? foregroundColor;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Padding(
      padding: const EdgeInsets.only(right: Spacings.m),
      child: GlassCircleButton(
        icon: AppIcon.x(
          size: 20,
          color: foregroundColor ?? colors.text.primary,
        ),
        color: backgroundColor,
        hitTargetSize: 48,
        onPressed: onPressed,
      ),
    );
  }
}
