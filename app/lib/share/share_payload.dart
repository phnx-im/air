// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:flutter/services.dart';
import 'package:logging/logging.dart';

/// Channel to the native share host (the iOS share extension).
const shareChannel = MethodChannel('ms.air/share');

final _log = Logger('SharePayload');

/// The content shared into the app, handed over by the native share host.
class SharePayload {
  const SharePayload({
    this.text,
    this.attachments = const [],
    this.shareTargetIdentifier,
    this.droppedAttachments = 0,
  });

  /// Shared text or URL, if any
  final String? text;

  /// Shared files, already extracted into files owned by the host
  final List<UiSharedAttachment> attachments;

  /// Identifier of the direct share target the share was launched with, if
  /// any (the chat id donated to the OS)
  final String? shareTargetIdentifier;

  /// Number of shared items the host could not hand over, because it could
  /// not read them or they exceeded its copy limit
  final int droppedAttachments;

  bool get isEmpty => (text?.isEmpty ?? true) && attachments.isEmpty;
}

/// Fetches the shared content from the native share host.
Future<SharePayload> getSharePayload() async {
  try {
    final map = await shareChannel.invokeMapMethod<String, dynamic>(
      'getSharedPayload',
    );
    if (map == null) {
      return const SharePayload();
    }
    final attachments = (map['attachments'] as List?)
        .orEmpty()
        .whereType<Map<Object?, Object?>>()
        .map(
          (item) => switch (item['path']) {
            final String path => UiSharedAttachment(
              path: path,
              mimeType: item['mimeType'] as String?,
            ),
            _ => null,
          },
        )
        .nonNulls
        .toList();
    return SharePayload(
      text: map['text'] as String?,
      attachments: attachments,
      shareTargetIdentifier: map['shareTargetIdentifier'] as String?,
      droppedAttachments: map['droppedAttachments'] as int? ?? 0,
    );
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to get shared payload: '${e.message}'", e, stacktrace);
    return const SharePayload();
  }
}

/// Asks the native share host to dismiss the share UI.
///
/// `success` indicates whether the shared content was handed over to the
/// protocol layer (sent or queued).
Future<void> closeShareHost({required bool success}) async {
  try {
    await shareChannel.invokeMethod('close', {'success': success});
  } on PlatformException catch (e, stacktrace) {
    _log.severe("Failed to close share host: '${e.message}'", e, stacktrace);
  }
}

extension on List<dynamic>? {
  List<dynamic> orEmpty() => this ?? const [];
}
