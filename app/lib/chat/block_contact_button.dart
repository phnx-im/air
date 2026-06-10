// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/ds/components/modal/dialog.dart';
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
    return AppButton(
      onPressed: () => _block(context),
      label: loc.blockContactButton_text,
      type: AppButtonType.secondary,
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
