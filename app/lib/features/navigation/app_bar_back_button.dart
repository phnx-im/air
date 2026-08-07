// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
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
    return Padding(
      padding: const EdgeInsets.only(left: S.s16),
      child: Align(
        alignment: Alignment.centerLeft,
        child: ButtonIcon(
          variant: ButtonIconVariant.elevated,
          icon: AppIconType.arrowLeft,
          iconColor: foregroundColor,
          fill: backgroundColor,
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
