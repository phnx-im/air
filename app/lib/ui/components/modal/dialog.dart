// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

class AirDialog extends StatelessWidget {
  const AirDialog({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Dialog(
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Spacings.m),
      ),
      child: Container(
        constraints: const BoxConstraints(maxWidth: 340),
        padding: const EdgeInsets.only(
          left: Spacings.s,
          right: Spacings.s,
          top: Spacings.m,
          bottom: Spacings.s,
        ),
        child: child,
      ),
    );
  }
}

class AirDialogProgressTextButton extends HookWidget {
  const AirDialogProgressTextButton({
    super.key,
    required this.onPressed,
    this.style,
    this.progressColor,
    required this.child,
  });

  final Function(ValueNotifier<bool> inProgress) onPressed;
  final ButtonStyle? style;
  final Color? progressColor;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final inProgress = useState(false);

    return TextButton(
      onPressed: () => inProgress.value ? null : onPressed(inProgress),
      style: style,
      child: !inProgress.value
          ? child
          : SizedBox(
              width: 20,
              height: 20,
              child: CircularProgressIndicator(
                strokeWidth: 2,
                valueColor: progressColor != null
                    ? AlwaysStoppedAnimation<Color>(progressColor!)
                    : null,
                backgroundColor: Colors.transparent,
              ),
            ),
    );
  }
}

const airDialogButtonStyle = ButtonStyle(
  visualDensity: VisualDensity.compact,
  padding: WidgetStatePropertyAll(EdgeInsets.all(Spacings.sm)),
  shape: WidgetStatePropertyAll(
    RoundedRectangleBorder(
      borderRadius: BorderRadius.all(Radius.circular(Spacings.xs)),
    ),
  ),
);
