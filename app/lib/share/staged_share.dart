// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/platform/method_channel.dart';
import 'package:flutter/services.dart';
import 'package:logging/logging.dart';
import 'package:uuid/uuid.dart';

final _log = Logger('StagedShare');

/// Content handed over from the Android share activity to be staged in a
/// chat's composer, exactly like an in-app attachment pick.
///
/// The shared text goes into the composer's input field and each attachment
/// runs through the regular upload preview. The destination is either the
/// direct share target the share was launched with, or the chat the user
/// opens from the chat list, which shows a banner while a share is pending.
class StagedShare {
  const StagedShare({
    this.chatId,
    this.attachments = const [],
    this.text,
    this.droppedAttachments = 0,
  });

  /// Destination chat, or null when the user picks it from the chat list.
  final ChatId? chatId;

  /// Files extracted by the share activity. The main app owns them and
  /// deletes them after the upload or when the share is dropped.
  final List<UiSharedAttachment> attachments;

  /// Text shared when not sharing a file (could also be both).
  final String? text;

  /// Number of shared items the share activity could not hand over.
  final int droppedAttachments;
}

/// Parses a `sharedIntoChat` payload from the platform and forwards it to
/// [sharedIntoChatSink]. Invalid payloads are logged and dropped.
void dispatchSharedIntoChat(
  Map<Object?, Object?> arguments,
  StreamSink<StagedShare> sharedIntoChatSink,
) {
  ChatId? chatId;
  final chatIdStr = arguments['chatId'] as String?;
  if (chatIdStr != null) {
    try {
      chatId = ChatId(uuid: UuidValue.withValidation(chatIdStr));
    } on FormatException catch (e, stacktrace) {
      // A share launched from a stale direct share target still carries
      // content, so it degrades to picking the chat manually.
      _log.warning(
        "Invalid chatId in share payload: '$chatIdStr'",
        e,
        stacktrace,
      );
    }
  }
  final paths = (arguments['paths'] as List?)?.whereType<String>().toList();
  final mimeTypes = (arguments['mimeTypes'] as List?)
      ?.whereType<String>()
      .toList();
  final attachments = [
    for (final (index, path) in (paths ?? const <String>[]).indexed)
      UiSharedAttachment(
        path: path,
        // Parallel to the paths; empty when the provider reported no type.
        mimeType: switch (mimeTypes?.elementAtOrNull(index)) {
          final mimeType? when mimeType.isNotEmpty => mimeType,
          _ => null,
        },
      ),
  ];
  final text = arguments['text'] as String?;
  final droppedAttachments = arguments['dropped'] as int? ?? 0;
  if (attachments.isEmpty &&
      (text == null || text.isEmpty) &&
      droppedAttachments == 0) {
    return;
  }
  sharedIntoChatSink.add(
    StagedShare(
      chatId: chatId,
      attachments: attachments,
      text: text,
      droppedAttachments: droppedAttachments,
    ),
  );
}

/// Fetches the share handoff that launched the app on Android cold start,
/// if any.
Future<void> consumeInitialShare(
  StreamSink<StagedShare> sharedIntoChatSink,
) async {
  if (!Platform.isAndroid) return;
  try {
    final payload = await platform.invokeMapMethod<Object?, Object?>(
      'getInitialShare',
    );
    if (payload == null) return;
    dispatchSharedIntoChat(payload, sharedIntoChatSink);
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to get initial share: '${e.message}'", e, stacktrace);
  }
}
