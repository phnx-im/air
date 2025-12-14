// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/spacings.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/app_icon.dart';
import 'package:flutter/material.dart';

class AppBarPlusButton extends StatelessWidget {
  const AppBarPlusButton({super.key, this.onPressed});

  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Padding(
      padding: const EdgeInsets.only(right: Spacings.m),
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onPressed,
          customBorder: const CircleBorder(),
          overlayColor: WidgetStateProperty.all(Colors.transparent),
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: colors.backgroundBase.secondary,
              shape: BoxShape.circle,
            ),
            child: SizedBox.square(
              dimension: 24,
              child: Center(
                child: AppIcon(
                  type: AppIconType.plus,
                  size: 16,
                  color: colors.text.primary,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
