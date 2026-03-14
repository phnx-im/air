// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/icons.dart';
import 'package:flutter/material.dart';
import 'package:logging/logging.dart';

/// Shows an error banner in the global scaffold messenger.
///
/// This function does not require a [BuildContext] to show an error banner.
void showErrorBannerStandalone(
  String Function(AppLocalizations) errorDescription,
) {
  scaffoldMessengerKey.currentState?.removeCurrentMaterialBanner.call();

  final context = scaffoldMessengerKey.currentContext;
  if (context == null) {
    Logger.detached(
      'showErrorBanner',
    ).severe("No context when showing error banner");
    return;
  }

  final colors = CustomColorScheme.of(context);
  final loc = AppLocalizations.of(context);

  scaffoldMessengerKey.currentState?.showMaterialBanner(
    MaterialBanner(
      backgroundColor: colors.function.danger,
      elevation: 0,
      dividerColor: Colors.transparent,
      leading: AppIcon.circleAlert(size: 32, color: colors.function.white),
      padding: const EdgeInsets.all(20),
      content: Text(
        errorDescription(loc),
        style: TextStyle(color: colors.function.white),
      ),
      actions: [
        Builder(
          builder: (context) {
            return TextButton(
              child: Text(
                loc.errorBanner_ok,
                style: TextStyle(color: colors.function.white),
              ),
              onPressed: () {
                ScaffoldMessenger.of(context).hideCurrentMaterialBanner();
              },
            );
          },
        ),
      ],
    ),
  );
}

/// Shows a snackbar in the global scaffold messenger.
///
/// This function does not require a [BuildContext] to show a snackbar.
void showSnackBarStandalone(SnackBar Function(AppLocalizations) snackBar) {
  scaffoldMessengerKey.currentState?.removeCurrentSnackBar();

  final context = scaffoldMessengerKey.currentContext;
  if (context == null) {
    Logger.detached('showSnackBar').severe("No context when showing snackbar");
    return;
  }

  final loc = AppLocalizations.of(context);
  scaffoldMessengerKey.currentState?.showSnackBar(snackBar(loc));
}
