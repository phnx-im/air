// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/snackbar/snackbar.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
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

  final palette = SemanticPalette.of(context);
  final loc = AppLocalizations.of(context);

  scaffoldMessengerKey.currentState?.showMaterialBanner(
    MaterialBanner(
      backgroundColor: palette.function.danger,
      elevation: 0,
      dividerColor: Colors.transparent,
      leading: AppIcon.circleAlert(
        size: 32,
        color: palette.function.neutral.white,
      ),
      padding: const EdgeInsets.all(20),
      content: Text(
        errorDescription(loc),
        style: TextStyle(color: palette.function.neutral.white),
      ),
      actions: [
        Builder(
          builder: (context) {
            return TextButton(
              child: Text(
                loc.errorBanner_ok,
                style: TextStyle(color: palette.function.neutral.white),
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
void showSnackBarStandalone(
  SnackBar Function(AppLocalizations) snackBar, {
  SnackbarTone tone = SnackbarTone.neutral,
}) {
  scaffoldMessengerKey.currentState?.removeCurrentSnackBar();

  final context = scaffoldMessengerKey.currentContext;
  if (context == null) {
    Logger.detached('showSnackBar').severe("No context when showing snackbar");
    return;
  }

  final loc = AppLocalizations.of(context);
  scaffoldMessengerKey.currentState?.showSnackBar(_asPill(snackBar(loc), tone));
}

/// Carries [source] in the design system pill: the platform snackbar keeps the
/// queueing, the timer, and the swipe-to-dismiss, while its own chrome is
/// stripped back to nothing so the pill paints the fill, the shadow, and the
/// label.
///
/// A snackbar whose content is not plain text keeps the platform look, since
/// the pill only carries a label.
SnackBar _asPill(SnackBar source, SnackbarTone tone) {
  final label = switch (source.content) {
    Text(:final data?) => data,
    _ => null,
  };
  if (label == null) return source;

  const tokens = SnackbarTokens.standard;
  return SnackBar(
    // The carrier hands its content the full width, so center the pill in it
    // rather than letting it stretch. The height factor keeps the carrier
    // wrapped around the pill instead of the viewport.
    content: Align(
      heightFactor: 1,
      child: Snackbar(tokens: tokens, label: label, tone: tone),
    ),
    duration: source.duration,
    backgroundColor: Colors.transparent,
    elevation: 0,
    behavior: .floating,
    padding: EdgeInsets.zero,
    margin: tokens.insets,
    // The carrier clips to its own bounds by default, which would cut the
    // pill's drop shadow.
    clipBehavior: .none,
  );
}
