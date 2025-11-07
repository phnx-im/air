// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
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

    return OutlinedButton(
      onPressed: () => _onPressed(context),
      child: Text(
        loc.reportSpamButton_text,
        style: TextStyle(color: CustomColorScheme.of(context).function.danger),
      ),
    );
  }

  void _onPressed(BuildContext context) async {
    final confirmed = await showDialog(
      context: context,
      builder: (BuildContext context) {
        final loc = AppLocalizations.of(context);

        return AlertDialog(
          title: Text(loc.reportSpamDialog_title),
          content: Text(loc.reportSpamDialog_content),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(false),
              child: Text(loc.reportSpamDialog_cancel),
            ),
            TextButton(
              onPressed: () => Navigator.of(context).pop(true),
              child: Text(loc.reportSpamDialog_reportSpam),
            ),
          ],
        );
      },
    );

    if (confirmed && context.mounted) {
      final loc = AppLocalizations.of(context);
      try {
        await context.read<UserCubit>().reportSpam(userId);
        if (context.mounted) {
          ScaffoldMessenger.of(
            context,
          ).showSnackBar(SnackBar(content: Text(loc.reportSpamDialog_success)));
        }
      } catch (e) {
        _log.severe("Failed to report spam: $e");
        if (context.mounted) {
          showErrorBanner(context, loc.reportSpamDialog_error);
        }
      }
    }
  }
}
