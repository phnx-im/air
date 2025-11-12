// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/services.dart';
import 'package:logging/logging.dart';
import 'package:permission_handler/permission_handler.dart';

import 'platform.dart';

final _log = Logger('NotificationPermissions');

Future<void> requestNotificationPermissionsIfNeeded() async {
  if (Platform.isMacOS) {
    // macOS: Use custom method channel
    _log.info("Requesting notification permission for macOS");
    try {
      final granted = await requestNotificationPermission();
      _log.info("macOS notification permission granted: $granted");
    } on PlatformException catch (e) {
      _log.severe(
        "System error requesting macOS notification permission: ${e.message}",
      );
    }
  } else if (Platform.isAndroid || Platform.isIOS) {
    // Mobile: Use permission_handler
    var status = await Permission.notification.status;
    switch (status) {
      case PermissionStatus.denied:
        _log.info("Notification permission denied, will ask the user");
        var requestStatus = await Permission.notification.request();
        _log.fine("The status is $requestStatus");
        break;
      default:
        _log.info("Notification permission status: $status");
    }
  }
}
