// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/platform/method_channel.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';
import 'package:path/path.dart' as p;
import 'package:share_plus/share_plus.dart';

final _log = Logger('AttachmentActions');

/// Writes [attachment] to wherever the platform keeps files the user owns, and
/// confirms it. iOS has no such place, so it shares instead, see
/// [shareAttachments].
Future<void> saveAttachment(
  BuildContext context,
  UiAttachment attachment,
) async {
  if (Platform.isAndroid) {
    // Android uses platform-specific code to write data directly into a
    // provided URI
    final attachmentsRepository = context.read<AttachmentsRepository>();
    final data = await attachmentsRepository.loadAttachment(
      attachmentId: attachment.attachmentId,
    );
    if (data == null) {
      _log.severe("Missing attachment data");
      return;
    }
    await saveFileAndroid(
      fileName: p.basename(attachment.filename),
      mimeType: attachment.contentType,
      data: data,
    );
  } else if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
    // On Desktop, we save the attachment in Rust after getting a path from the
    // platform-specific dialog
    final attachmentsRepository = context.read<AttachmentsRepository>();
    final location = await getSaveLocation(
      suggestedName: p.basename(attachment.filename),
    );
    if (location == null) return;

    try {
      await attachmentsRepository.saveAttachment(
        attachmentId: attachment.attachmentId,
        path: location.path,
      );
    } catch (e, stackTrace) {
      _log.severe("Failed to save attachment: $e", e, stackTrace);
      showErrorBannerStandalone((loc) => loc.messageContextMenu_saveError);
      return;
    }
  } else if (Platform.isIOS) {
    throw UnsupportedError("iOS does not support storing files");
  } else {
    throw UnsupportedError("Unsupported platform");
  }

  showSnackBarStandalone(
    (loc) => SnackBar(
      duration: const Duration(seconds: 1),
      content: Text(loc.messageContextMenu_saveConfirmation),
    ),
  );
}

/// Hands [attachments] to the platform share sheet. Anything that fails to load
/// is left out rather than failing the whole share.
Future<void> shareAttachments(
  BuildContext context,
  List<UiAttachment> attachments,
) async {
  final attachmentsRepository = context.read<AttachmentsRepository>();

  final futures = attachments.map((attachment) async {
    final data = await attachmentsRepository.loadAttachment(
      attachmentId: attachment.attachmentId,
    );
    if (data == null) return null;
    return XFile.fromData(data);
  });

  final files = (await Future.wait(futures)).whereType<XFile>().toList();

  final params = ShareParams(
    files: files,
    fileNameOverrides: attachments.map((e) => e.filename).toList(),
  );
  SharePlus.instance.share(params);
}
