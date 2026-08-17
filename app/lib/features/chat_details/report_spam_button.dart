// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

final _log = Logger("ReportSpamButton");

class ReportSpamButton extends StatelessWidget {
  const ReportSpamButton({required this.userId, super.key});

  final UiUserId userId;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Button(
      onPressed: () => _onPressed(context),
      size: ButtonSize.current,
      type: ButtonType.secondary,
      label: loc.reportSpamButton_text,
    );
  }

  void _onPressed(BuildContext context) async {
    final loc = AppLocalizations.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => ConfirmDialog(
        title: loc.reportSpamDialog_title,
        message: loc.reportSpamDialog_content,
        cancel: loc.reportSpamDialog_cancel,
        confirm: loc.reportSpamDialog_reportSpam,
        destructive: true,
      ),
    );

    if ((confirmed ?? false) && context.mounted) {
      try {
        await context.read<UserCubit>().reportSpam(userId);
        showSnackBarStandalone(
          (loc) => SnackBar(content: Text(loc.reportSpamDialog_success)),
        );
      } catch (e) {
        _log.severe("Failed to report spam: $e");
        showErrorBannerStandalone((loc) => loc.reportSpamDialog_error);
      }
    }
  }
}
