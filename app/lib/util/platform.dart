// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';
import 'package:flutter/services.dart';
import 'package:logging/logging.dart';
import 'package:air/core/core.dart';
import 'package:path_provider/path_provider.dart';
import 'package:uuid/uuid.dart';

const platform = MethodChannel('ms.air/channel');

final _log = Logger('Platform');

void initMethodChannel(StreamSink<ChatId> openedNotificationSink) {
  platform.setMethodCallHandler(
    (call) => _handleMethod(call, openedNotificationSink),
  );
}

Future<void> _handleMethod(
  MethodCall call,
  StreamSink<ChatId> openedNotificationSink,
) async {
  _log.info('Handling method call: ${call.method}');
  switch (call.method) {
    case 'receivedNotification':
      // Handle notification data
      final String? identifier = call.arguments["identifier"];
      final String? chatIdStr = call.arguments["chatId"];
      _log.info(
        'Received notification: identifier = $identifier, chatId = $chatIdStr',
      );
      // Do something with the data
      break;
    case 'openedNotification':
      // Handle notification opened
      final String? identifier = call.arguments["identifier"];
      final String? chatIdStr = call.arguments["chatId"];
      _log.fine(
        'Notification opened: identifier = $identifier, chatId = $chatIdStr',
      );
      if (identifier != null && chatIdStr != null) {
        final chatId = ChatId(uuid: UuidValue.withValidation(chatIdStr));
        openedNotificationSink.add(chatId);
      }
      break;
    case 'backgroundTaskExpired':
      final taskId = call.arguments['taskId'];
      _log.warning('Background task expired (taskId=$taskId)');
      break;
    default:
      _log.severe('Unknown method called: ${call.method}');
  }
}

Future<String?> getDeviceToken() async {
  if (Platform.isAndroid || Platform.isIOS) {
    try {
      return await platform.invokeMethod('getDeviceToken');
    } on PlatformException catch (e, stacktrace) {
      _log.severe("Failed to get device token: '${e.message}'.", e, stacktrace);
    }
  }
  return null;
}

Future<String> getDatabaseDirectoryMobile() async {
  if (Platform.isAndroid || Platform.isIOS) {
    try {
      return await platform.invokeMethod('getDatabasesDirectory');
    } on PlatformException catch (e, stacktrace) {
      _log.severe(
        "Failed to get database directory: '${e.message}'",
        e,
        stacktrace,
      );
      throw PlatformException(code: 'failed_to_get_database_directory');
    }
  }
  return '';
}

/// Saves file data to the appropriate public directory on Android.
Future<void> saveFileAndroid({
  required String fileName,
  required String mimeType,
  required Uint8List data,
}) async {
  if (!Platform.isAndroid) {
    throw PlatformException(code: 'unsupported_platform');
  }
  try {
    await platform.invokeMethod('saveFile', {
      'fileName': fileName,
      'mimeType': mimeType,
      'data': data,
    });
  } on PlatformException catch (e) {
    _log.severe("Failed to save file: '${e.message}'.");
  }
}

/// Returns the directory returned by `getApplicationDocumentsDirectory` on all platforms, except for
/// iOS. On iOS, the directory returned is the `Caches` directory in a shared container of the
/// application group. The container is shared between the application and background extension.
Future<String> getCacheDirectory() async {
  return Platform.isIOS
      ? (await _getSharedCacheDirectoryIOS())!
      : (await getApplicationCacheDirectory()).path;
}

Future<String?> _getSharedCacheDirectoryIOS() async {
  if (Platform.isIOS) {
    try {
      return await platform.invokeMethod('getSharedCacheDirectory');
    } on PlatformException catch (e, stacktrace) {
      _log.severe(
        "Failed to get shared cache directory: '${e.message}'",
        e,
        stacktrace,
      );
      throw PlatformException(code: 'failed_to_get_shared_cache_directory');
    }
  }
  return null;
}

Future<void> setBadgeCount(int count) async {
  // Make sure we are on iOS or macOS
  if (!Platform.isIOS && !Platform.isMacOS) {
    return;
  }
  try {
    await platform.invokeMethod('setBadgeCount', {'count': count});
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to set badge count: '${e.message}'.", e, stacktrace);
  }
}

Future<int?> beginBackgroundTask() async {
  if (!Platform.isIOS) return null;
  try {
    final result = await platform.invokeMethod('beginBackgroundTask');
    if (result is int) return result;
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to begin background task: '${e.message}'.", e, stacktrace);
  }
  return null;
}

Future<void> endBackgroundTask(int? taskId) async {
  if (!Platform.isIOS || taskId == null) return;
  try {
    await platform.invokeMethod('endBackgroundTask', {'taskId': taskId});
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to end background task: '${e.message}'.", e, stacktrace);
  }
}

Future<bool> requestNotificationPermission() async {
  if (!Platform.isMacOS) {
    return false;
  }
  try {
    final result = await platform.invokeMethod('requestNotificationPermission');
    if (result is bool) {
      return result;
    } else {
      _log.warning(
        "requestNotificationPermission returned unexpected type: ${result.runtimeType}",
      );
      return false;
    }
  } on PlatformException catch (e, stacktrace) {
    _log.severe(
      "Failed to request notification permission: '${e.message}'.",
      e,
      stacktrace,
    );
    // Re-throw the error so the caller knows there was a system error vs. permission denial
    rethrow;
  }
}

FutureOr<void> sendNotification(NotificationContent content) async {
  try {
    final arguments = <String, dynamic>{
      'identifier': content.identifier.field0.toString(),
      'title': content.title,
      'body': content.body,
      'chatId': content.chatId?.uuid.toString(),
    };
    await platform.invokeMethod('sendNotification', arguments);
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to send notifications: '${e.message}'", e, stacktrace);
  }
}

FutureOr<List<NotificationHandle>> getActiveNotifications() async {
  try {
    List<Map<Object?, Object?>> res =
        await platform.invokeListMethod('getActiveNotifications') ?? [];
    return res.map(NotificationHandleExtension.fromMap).nonNulls.toList();
  } on PlatformException catch (e, stacktrace) {
    _log.severe(
      "Failed to get active notifications: '${e.message}'",
      e,
      stacktrace,
    );
  }
  return [];
}

FutureOr<void> cancelNotifications(List<NotificationId> identifiers) async {
  if (identifiers.isEmpty) {
    return;
  }
  try {
    final arguments = <String, dynamic>{
      'identifiers': identifiers.map((id) => id.field0.toString()).toList(),
    };
    await platform.invokeMethod('cancelNotifications', arguments);
  } on PlatformException catch (e, stacktrace) {
    _log.severe(
      "Failed to cancel notifications: '${e.message}'",
      e,
      stacktrace,
    );
  }
}
