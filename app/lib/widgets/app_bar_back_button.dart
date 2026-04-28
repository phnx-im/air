// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/navigation/navigation.dart';
import 'package:air/theme/spacings.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/button/glass_circle_button.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class AppBarBackButton extends StatelessWidget {
  const AppBarBackButton({
    super.key,
    this.foregroundColor,
    this.backgroundColor,
  });

  final Color? foregroundColor;
  final Color? backgroundColor;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Padding(
      padding: const EdgeInsets.only(left: Spacings.s),
      child: Align(
        alignment: Alignment.centerLeft,
        child: GlassCircleButton(
          icon: AppIcon.arrowLeft(
            size: 20,
            color: foregroundColor ?? colors.text.primary,
          ),
          color: backgroundColor,
          hitTargetSize: 48,
          onPressed: () async {
            final navigator = Navigator.of(context);
            final popped = await navigator.maybePop();
            if (!popped && context.mounted) {
              context.read<NavigationCubit>().pop();
            }
          },
        ),
      ),
    );
  }
}
