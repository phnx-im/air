// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/user.dart';
import 'package:air/util/dialog.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class UnblockContactButton extends StatelessWidget {
  const UnblockContactButton({
    required this.userId,
    required this.displayName,
    super.key,
  });

  final UiUserId userId;
  final String displayName;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return OutlinedButton(
      onPressed: () => _unblock(context),
      child: Text(loc.unblockContactButton_text),
    );
  }

  void _unblock(BuildContext context) async {
    final userCubit = context.read<UserCubit>();
    final loc = AppLocalizations.of(context);
    final confirmed = await showConfirmationDialog(
      context,
      title: loc.unblockContactDialog_title(displayName),
      message: loc.unblockContactDialog_content(displayName),
      positiveButtonText: loc.unblockContactDialog_unblock,
      negativeButtonText: loc.unblockContactDialog_cancel,
    );
    if (confirmed) {
      userCubit.unblockContact(userId);
    }
  }
}
