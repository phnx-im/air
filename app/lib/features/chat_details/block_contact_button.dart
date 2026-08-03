// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/ds/patterns/dialog/show_confirmation_dialog.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class BlockContactButton extends StatelessWidget {
  const BlockContactButton({
    required this.userId,
    required this.displayName,
    super.key,
  });

  final UiUserId userId;
  final String displayName;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final palette = SemanticPalette.of(context);

    final isDesktop = DeviceType.isDesktop;

    return OutlinedButton(
      onPressed: () => _block(context),
      style: ButtonStyle(
        minimumSize: WidgetStatePropertyAll(
          Size(isDesktop ? 320 : double.infinity, 0),
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        spacing: S.s12,
        children: [
          Text(
            loc.blockContactButton_text,
            style: typeScale.body.regular.style(color: palette.text.primary),
          ),
        ],
      ),
    );
  }

  void _block(BuildContext context) async {
    final userCubit = context.read<UserCubit>();
    final loc = AppLocalizations.of(context);
    final confirmed = await showConfirmationDialog(
      context,
      title: loc.blockContactDialog_title(displayName),
      message: loc.blockContactDialog_content(displayName),
      positiveButtonText: loc.blockContactDialog_block,
      negativeButtonText: loc.blockContactDialog_cancel,
    );
    if (confirmed) {
      userCubit.blockContact(userId);
    }
  }
}
