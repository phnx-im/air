// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

/// Shows a confirmation dialog with the given [title], [message],
/// [positiveButtonText] and [negativeButtonText].
///
/// Returns true if the user confirmed the action, false otherwise.
Future<bool> showConfirmationDialog(
  BuildContext context, {
  required String title,
  required String message,
  required String positiveButtonText,
  required String negativeButtonText,
}) async {
  bool confirmed = await showDialog(
    context: context,
    builder: (BuildContext context) {
      return AlertDialog(
        title: Text(title),
        content: Text(message),
        actions: [
          TextButton(
            onPressed: () {
              Navigator.of(context).pop(false);
            },
            child: Text(negativeButtonText),
          ),
          TextButton(
            onPressed: () {
              Navigator.of(context).pop(true);
            },
            child: Text(positiveButtonText),
          ),
        ],
      );
    },
  );
  return confirmed;
}
