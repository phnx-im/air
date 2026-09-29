// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/patterns/snackbar/snackbar.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:flutter/material.dart';
import 'package:logging/logging.dart';

final _log = Logger('ScaffoldMessenger');

/// The global messenger's only root [Scaffold].
///
/// It should be installed directly above the navigator so snackbars show above
/// every route, including modals and dialogs.
class RootScaffold extends StatelessWidget {
  const RootScaffold({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.transparent,
      resizeToAvoidBottomInset: false,
      body: child,
    );
  }
}

/// Shows a snackbar in the global scaffold messenger.
///
/// This function does not require a [BuildContext] to show a snackbar. The
/// label is resolved in the app's locale when the snackbar is shown.
void showSnackBarStandalone(
  String Function(AppLocalizations) label, {
  required SnackbarTone tone,
  Duration duration = SnackbarTokens.duration,
}) {
  final messenger = scaffoldMessengerKey.currentState;
  if (messenger == null) {
    _log.severe("No messenger when showing a snackbar");
    return;
  }
  messenger.removeCurrentSnackBar();
  messenger.showSnackBar(
    SnackBar(
      content: Builder(
        builder: (context) => Padding(
          // RootScaffold does not resize to avoid the bottom inset, so the
          // pill needs to be offset by it.
          padding: EdgeInsets.only(
            bottom: MediaQuery.viewInsetsOf(context).bottom,
          ),
          // The carrier hands its content the full width, so center the pill
          // in it rather than letting it stretch. The height factor keeps the
          // carrier wrapped around the pill instead of the viewport.
          child: Align(
            heightFactor: 1,
            child: Snackbar(
              label: label(AppLocalizations.of(context)),
              tone: tone,
            ),
          ),
        ),
      ),
      duration: duration,
      backgroundColor: Colors.transparent,
      elevation: 0,
      behavior: .floating,
      padding: EdgeInsets.zero,
      margin: SnackbarTokens.insets,
      // The carrier clips to its own bounds by default, which would cut the
      // pill's drop shadow.
      clipBehavior: .none,
    ),
  );
}
