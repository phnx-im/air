// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
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

    final isDesktop = DeviceType.isDesktop;

    return OutlinedButton(
      onPressed: () => _onPressed(context),
      style: ButtonStyle(
        minimumSize: WidgetStatePropertyAll(
          Size(isDesktop ? 320 : double.infinity, 0),
        ),
      ),
      child: Text(
        loc.reportSpamButton_text,
        style: typeScale.body.regular.style(
          color: SemanticColors.of(context).text.primary,
        ),
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
