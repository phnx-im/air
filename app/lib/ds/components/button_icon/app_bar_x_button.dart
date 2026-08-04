// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:flutter/widgets.dart';

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
    return Padding(
      padding: const EdgeInsets.only(right: S.s24),
      child: ButtonIcon(
        variant: ButtonIconVariant.elevated,
        icon: AppIconType.x,
        iconColor: foregroundColor,
        fill: backgroundColor,
        hitTargetSize: 48,
        onPressed: onPressed,
      ),
    );
  }
}
