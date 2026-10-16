// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:air/ds/patterns/dialog/dialog_tokens.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';

/// A dialog for confirming a single action.
class ConfirmDialog extends StatelessWidget {
  const ConfirmDialog({
    super.key,

    required this.title,
    this.message,
    this.cancel,
    required this.confirm,

    this.onConfirm,
    this.destructive = false,
  });

  final String title;

  /// What the title leaves to say. A dialog whose title asks the whole question
  /// leaves it out rather than repeating itself.
  final String? message;
  final String? cancel;
  final String confirm;

  final VoidCallback? onConfirm;
  final bool destructive;

  @override
  Widget build(BuildContext context) {
    final tokens = DialogTokens.current;
    final palette = SemanticPalette.of(context);
    final cancel = this.cancel;
    final message = this.message;

    return AppDialog(
      child: Column(
        mainAxisSize: .min,
        children: [
          Text(
            title,
            textAlign: .center,
            style: typeScale.header.regular.style(
              color: palette.text.primary,
              weight: Weight.emphasized,
            ),
          ),

          if (message != null) ...[
            SizedBox(height: tokens.titleBodyGap),

            Text(
              message,
              textAlign: .center,
              style: typeScale.body.regular.style(
                color: palette.text.secondary,
              ),
            ),
          ],

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
                    // Pop first, so a dialog shown by onConfirm stays open.
                    Navigator.of(context).pop(true);
                    onConfirm?.call();
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

/// Show a modal dialog with an error message.
///
/// Without a [title], it reads "Something went wrong".
void showErrorDialog(
  BuildContext context, {
  String Function(AppLocalizations)? title,
  required String Function(AppLocalizations) message,
}) {
  showDialog(
    context: context,
    builder: (context) {
      final loc = AppLocalizations.of(context);
      return ConfirmDialog(
        title: title != null ? title(loc) : loc.errorDialog_title,
        message: message(loc),
        confirm: loc.errorDialog_confirm,
      );
    },
  );
}
