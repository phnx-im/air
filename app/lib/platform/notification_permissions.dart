// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/services.dart';
import 'package:logging/logging.dart';

import 'package:air/platform/method_channel.dart';

final _log = Logger('NotificationPermissions');

Future<void> requestNotificationPermission() async {
  if (!Platform.isIOS && !Platform.isAndroid && !Platform.isMacOS) {
    return;
  }
  try {
    final granted = await platform.invokeMethod<bool>(
      'requestNotificationPermission',
    );
    _log.info("Notification permission granted: $granted");
  } on PlatformException catch (e, stacktrace) {
    _log.severe(
      "Failed to request notification permission: '${e.message}'",
      e,
      stacktrace,
    );
  }
}
