// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

class AppDialog extends StatelessWidget {
  const AppDialog({
    super.key,
    this.backgroundColor,
    this.maxWidth,
    required this.child,
  });

  final Widget child;
  final Color? backgroundColor;
  final double? maxWidth;

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: backgroundColor,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Spacings.m),
      ),
      child: Container(
        constraints: BoxConstraints(maxWidth: maxWidth ?? 340),
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

/// Same as [AppDialog] but can be used outside of a [Dialog] context.
class AppDialogContainer extends StatelessWidget {
  const AppDialogContainer({
    super.key,
    this.backgroundColor,
    this.maxWidth,
    required this.child,
  });

  final Widget child;
  final Color? backgroundColor;
  final double? maxWidth;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Container(
        constraints: const BoxConstraints(maxWidth: 360),
        decoration: BoxDecoration(
          color: backgroundColor,
          borderRadius: BorderRadius.circular(Spacings.m),
        ),
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

class AppDialogProgressButton extends HookWidget {
  const AppDialogProgressButton({
    super.key,
    this.onPressed,
    this.style,
    this.progressColor,
    this.inProgress,
    required this.child,
  });

  final Function(ValueNotifier<bool> inProgress)? onPressed;
  final ButtonStyle? style;
  final Color? progressColor;
  final Widget child;
  final bool? inProgress;

  @override
  Widget build(BuildContext context) {
    final inProgress = useState(this.inProgress ?? false);

    return OutlinedButton(
      onPressed: onPressed != null
          ? () => inProgress.value ? null : onPressed?.call(inProgress)
          : null,
      style: style,
      child: !inProgress.value
          ? child
          : SizedBox(
              width: 16,
              height: 16,
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

const appDialogInputDecoration = InputDecoration(
  contentPadding: EdgeInsets.symmetric(
    horizontal: Spacings.xxs,
    vertical: Spacings.xxs,
  ),
  isDense: true,
  border: _outlineInputBorder,
  enabledBorder: _outlineInputBorder,
  focusedBorder: _outlineInputBorder,
);

const _outlineInputBorder = OutlineInputBorder(
  borderRadius: BorderRadius.all(Radius.circular(Spacings.s)),
  borderSide: BorderSide(width: 0, style: BorderStyle.none),
);
