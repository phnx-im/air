// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
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

    return Button(
      onPressed: () => _block(context),
      size: ButtonSize.of(context),
      type: ButtonType.secondary,
      tone: ButtonTone.danger,
      label: loc.blockContactButton_text,
    );
  }

  void _block(BuildContext context) async {
    final userCubit = context.read<UserCubit>();
    final loc = AppLocalizations.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => ConfirmDialog(
        title: loc.blockContactDialog_title(displayName),
        message: loc.blockContactDialog_content(displayName),
        cancel: loc.blockContactDialog_cancel,
        confirm: loc.blockContactDialog_block,
        destructive: true,
      ),
    );
    if (confirmed ?? false) {
      userCubit.blockContact(userId);
    }
  }
}
