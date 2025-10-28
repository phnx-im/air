// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// A button that shows a progress indicator when pressed
class ProgressButton extends HookWidget {
  const ProgressButton({
    super.key,
    this.onPressed,
    this.style,
    required this.label,
  });

  final Function(ValueNotifier<bool>)? onPressed;
  final String label;
  final ButtonStyle? style;

  @override
  Widget build(BuildContext context) {
    final inProgress = useState(false);

    final theme = Theme.of(context);
    final buttonTheme = theme.outlinedButtonTheme;
    final resolvedTextStyle =
        buttonTheme.style?.textStyle?.resolve({}) ?? theme.textTheme.labelLarge;
    final buttonFontSize = resolvedTextStyle?.fontSize ?? 14.0;

    return OutlinedButton(
      style: style,
      onPressed:
          onPressed != null && !inProgress.value
              ? () {
                inProgress.value = true;
                onPressed!(inProgress);
              }
              : null,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (inProgress.value)
            SizedBox(
              width: buttonFontSize,
              height: buttonFontSize,
              child: CircularProgressIndicator(
                valueColor: AlwaysStoppedAnimation<Color>(
                  CustomColorScheme.of(context).text.secondary,
                ),
                backgroundColor: Colors.transparent,
              ),
            ),
          if (inProgress.value) const SizedBox(width: Spacings.s),
          Text(label),
        ],
      ),
    );
  }
}
