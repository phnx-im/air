// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/button_icon/glass_circle_button.dart';
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
    final palette = SemanticPalette.of(context);

    return Padding(
      padding: const EdgeInsets.only(right: S.s24),
      child: GlassCircleButton(
        icon: AppIcon.x(
          size: 20,
          color: foregroundColor ?? palette.text.primary,
        ),
        color: backgroundColor,
        hitTargetSize: 48,
        onPressed: onPressed,
      ),
    );
  }
}
