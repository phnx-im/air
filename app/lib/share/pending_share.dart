// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/platform/method_channel.dart';
import 'package:flutter/services.dart';
import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:logging/logging.dart';
import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

part 'pending_share.freezed.dart';

final _log = Logger('PendingShare');

/// Mirrors `ShareActivity.SHARE_CACHE_DIR`.
const _shareCacheDir = 'share';

/// Content handed over from the Android share activity, waiting for a chat's
/// composer to take it, exactly like an in-app attachment pick.
///
/// The shared text goes into the composer's input field and each attachment
/// runs through the regular upload preview. Where it goes is navigation
/// state, not part of this: see `HomeNavigationState.pendingShare`.
@freezed
abstract class PendingShare with _$PendingShare {
  const PendingShare._();

  const factory PendingShare({
    /// Files extracted by the share activity. Whoever holds the share owns
    /// them: the composer once it stages them, the navigation cubit until
    /// then.
    @Default(<UiSharedAttachment>[]) List<UiSharedAttachment> attachments,

    /// Text shared when not sharing a file (could also be both).
    String? text,

    /// Number of shared items the share activity could not hand over.
    @Default(0) int droppedAttachments,
  }) = _PendingShare;

  Future<void> deleteFiles() => deleteShareFiles(attachments);
}

/// A share and the chat the share activity launched it for, if any.
typedef ShareHandoff = ({PendingShare share, ChatId? chatId});

/// Best-effort delete of files the share activity extracted.
///
/// Each file sits in a directory of its own (`cacheDir/share/<uuid>/`), which
/// goes with it. A file from anywhere else only loses itself. What is left
/// behind anyway is swept by the share activity's `deleteStaleCacheDirs`.
Future<void> deleteShareFiles(Iterable<UiSharedAttachment> attachments) async {
  for (final attachment in attachments) {
    final file = File(attachment.path);
    final dir = file.parent;
    final ownsDir = p.basename(dir.parent.path) == _shareCacheDir;
    try {
      await (ownsDir ? dir.delete(recursive: true) : file.delete());
    } catch (e) {
      _log.warning("Failed to delete dropped share file: $e");
    }
  }
}

/// Parses a `sharedIntoChat` payload from the platform and forwards it to
/// [handoffSink]. Invalid payloads are logged and dropped.
void dispatchSharedIntoChat(
  Map<Object?, Object?> arguments,
  StreamSink<ShareHandoff> handoffSink,
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
  handoffSink.add((
    share: PendingShare(
      attachments: attachments,
      text: text,
      droppedAttachments: droppedAttachments,
    ),
    chatId: chatId,
  ));
}

/// Fetches the share handoff that launched the app on Android cold start,
/// if any.
Future<void> consumeInitialShare(StreamSink<ShareHandoff> handoffSink) async {
  if (!Platform.isAndroid) return;
  try {
    final payload = await platform.invokeMapMethod<Object?, Object?>(
      'getInitialShare',
    );
    if (payload == null) return;
    dispatchSharedIntoChat(payload, handoffSink);
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to get initial share: '${e.message}'", e, stacktrace);
  }
}
