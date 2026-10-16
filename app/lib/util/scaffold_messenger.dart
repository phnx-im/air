// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/patterns/snackbar/snackbar.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
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
      content: ValueListenableBuilder(
        valueListenable: _clearanceWidgets,
        builder: (context, clearanceWidgets, _) {
          final keyboard = MediaQuery.viewInsetsOf(context).bottom;
          return Padding(
            // RootScaffold does not resize for the keyboard. Clear it by the
            // composer's height while one is on top, and by a small gap
            // otherwise.
            padding: EdgeInsets.only(
              bottom: keyboard > 0 && clearanceWidgets == 0
                  ? keyboard + SnackbarTokens.keyboardGap
                  : keyboard + SnackbarTokens.insets.bottom,
            ),
            // The carrier hands its content the full width, so center the pill
            // in it rather than letting it stretch. The height factor keeps
            // the carrier wrapped around the pill instead of the viewport.
            child: Align(
              heightFactor: 1,
              child: Snackbar(
                label: label(AppLocalizations.of(context)),
                tone: tone,
              ),
            ),
          );
        },
      ),
      duration: duration,
      backgroundColor: Colors.transparent,
      elevation: 0,
      behavior: .floating,
      padding: EdgeInsets.zero,
      margin: SnackbarTokens.insets.copyWith(bottom: 0),
      // The carrier clips to its own bounds by default, which would cut the
      // pill's drop shadow.
      clipBehavior: .none,
    ),
  );
}

final _clearanceWidgets = ValueNotifier<int>(0);

/// Keeps the snackbar's full bottom clearance while the keyboard is up.
///
/// With the keyboard up, the snackbar sits [SnackbarTokens.keyboardGap] above
/// it. While this widget is mounted on the current route, it keeps
/// [SnackbarTokens.insets] bottom instead, which clears a [child] up to that
/// height.
class SnackBarClearance extends HookWidget {
  const SnackBarClearance({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final isCurrent = ModalRoute.isCurrentOf(context) ?? true;
    useEffect(() {
      if (!isCurrent) return null;
      _afterFrame(() => _clearanceWidgets.value++);
      return () => _afterFrame(() => _clearanceWidgets.value--);
    }, [isCurrent]);
    return child;
  }
}

void _afterFrame(VoidCallback callback) {
  WidgetsBinding.instance.addPostFrameCallback((_) => callback());
}
