// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:flutter/material.dart';

class AppBarPlusButton extends StatelessWidget {
  const AppBarPlusButton({super.key, this.onPressed});

  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(right: Spacing.px16),
      child: GlassCircleButton(
        icon: const AppIcon.plus(size: 20),
        hitTargetSize: 48,
        onPressed: onPressed,
      ),
    );
  }
}
