// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/util/dialog.dart';
import 'package:flutter/material.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
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
    final color = CustomColorScheme.of(context).function.danger;
    return OutlinedButton(
      onPressed: () => _block(context),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        spacing: Spacings.xs,
        children: [
          iconoir.Prohibition(width: 20, color: color),
          Text(loc.blockContactButton_text, style: TextStyle(color: color)),
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
