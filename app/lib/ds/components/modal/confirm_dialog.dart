// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/components/modal/app_dialog.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:flutter/material.dart';

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
    final colors = CustomColorScheme.of(context);
    final cancel = this.cancel;

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Text(
              title,
              style: TextStyle(
                fontSize: HeaderFontSize.h4.size,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),

          const SizedBox(height: Spacing.px8),

          Text(
            message,
            style: TextStyle(
              color: colors.text.secondary,
              fontSize: BodyFontSize.base.size,
            ),
          ),

          const SizedBox(height: Spacing.px24),

          Row(
            children: [
              if (cancel != null) ...[
                Expanded(
                  child: AppButton(
                    onPressed: () => Navigator.of(context).pop(false),
                    label: cancel,
                    type: .secondary,
                  ),
                ),

                const SizedBox(width: Spacing.px12),
              ],

              Expanded(
                child: AppButton(
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
