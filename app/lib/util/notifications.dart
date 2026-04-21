// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/util/platform.dart';
import 'package:flutter/services.dart';
import 'package:logging/logging.dart';
import 'package:uuid/uuid.dart';

final _log = Logger('NotificationTaps');

/// Drain any notification tap that launched the app on Android cold start.
Future<void> consumeInitialNotification(
  StreamSink<ChatId> openedNotificationSink,
) async {
  // Only needed for Android, since iOS does not support launching the app from
  // a notification tap
  if (!Platform.isAndroid) return;
  try {
    final payload = await platform.invokeMapMethod<String, Object?>(
      'getInitialNotification',
    );
    if (payload == null) return;
    dispatchOpenedNotification(payload, openedNotificationSink);
  } on PlatformException catch (e, stacktrace) {
    _log.severe(
      "Failed to get initial notification: '${e.message}'",
      e,
      stacktrace,
    );
  }
}

/// Parses an `openedNotification` payload from the platform and forwards the
/// resulting [ChatId] to [openedNotificationSink]. Invalid payloads are logged
/// and dropped.
void dispatchOpenedNotification(
  Map<Object?, Object?> arguments,
  StreamSink<ChatId> openedNotificationSink,
) {
  final identifier = arguments["identifier"] as String?;
  final chatIdStr = arguments["chatId"] as String?;
  _log.fine(
    'Notification opened: identifier = $identifier, chatId = $chatIdStr',
  );
  if (identifier == null || chatIdStr == null) return;
  try {
    openedNotificationSink.add(
      ChatId(uuid: UuidValue.withValidation(chatIdStr)),
    );
  } on FormatException catch (e, stacktrace) {
    _log.warning(
      "Invalid chatId in notification payload: '$chatIdStr'",
      e,
      stacktrace,
    );
  }
}
