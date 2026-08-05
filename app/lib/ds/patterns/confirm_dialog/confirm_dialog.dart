// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:air/ds/patterns/dialog/dialog_tokens.dart';
import 'package:flutter/widgets.dart';

/// A dialog for confirming a single action.
class ConfirmDialog extends StatelessWidget {
  const ConfirmDialog({
    super.key,

    required this.title,
    required this.message,
    this.cancel,
    required this.confirm,

    this.onConfirm,
    this.destructive = false,
  });

  final String title;
  final String message;
  final String? cancel;
  final String confirm;

  final VoidCallback? onConfirm;
  final bool destructive;

  @override
  Widget build(BuildContext context) {
    final tokens = DialogTokens.of(context);
    final palette = SemanticPalette.of(context);
    final cancel = this.cancel;

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            title,
            textAlign: TextAlign.center,
            style: typeScale.header.regular.style(
              color: palette.text.primary,
              weight: Weight.emphasized,
            ),
          ),

          SizedBox(height: tokens.titleBodyGap),

          Text(
            message,
            textAlign: TextAlign.center,
            style: typeScale.body.regular.style(color: palette.text.secondary),
          ),

          SizedBox(height: tokens.bodyActionsGap),

          Row(
            children: [
              if (cancel != null) ...[
                Expanded(
                  child: Button(
                    onPressed: () => Navigator.of(context).pop(false),
                    label: cancel,
                    type: .secondary,
                  ),
                ),

                const SizedBox(width: S.s12),
              ],

              Expanded(
                child: Button(
                  onPressed: () {
                    onConfirm?.call();
                    Navigator.of(context).pop(true);
                  },
                  label: confirm,
                  type: .primary,
                  tone: destructive ? .danger : .normal,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
