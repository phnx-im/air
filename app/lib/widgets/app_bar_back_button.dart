// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/navigation/navigation.dart';
import 'package:air/theme/spacings.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
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
    final foregroundColor = this.foregroundColor ?? colors.text.primary;
    final backgroundColor =
        this.backgroundColor ?? colors.backgroundBase.secondary;

    return Padding(
      padding: const EdgeInsets.only(left: Spacings.s),
      child: Align(
        alignment: Alignment.centerLeft,
        child: Material(
          child: InkWell(
            customBorder: const CircleBorder(),
            overlayColor: WidgetStateProperty.all(Colors.transparent),
            onTap: () async {
              final navigator = Navigator.of(context);
              final popped = await navigator.maybePop();
              if (!popped && context.mounted) {
                context.read<NavigationCubit>().pop();
              }
            },
            child: Container(
              width: 32,
              height: 32,
              decoration: BoxDecoration(
                color: backgroundColor,
                shape: BoxShape.circle,
              ),
              child: Center(
                child: iconoir.ArrowLeft(
                  width: 16,
                  height: 16,
                  color: foregroundColor,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
