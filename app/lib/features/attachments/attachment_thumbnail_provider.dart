// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:ui' as ui;

import 'package:air/core/core.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/painting.dart';

/// Loads the locally generated thumbnail for an image attachment.
class AttachmentThumbnailProvider extends ImageProvider<AttachmentId> {
  const AttachmentThumbnailProvider({
    required this.attachmentId,
    required this.attachmentsRepository,
  });

  final AttachmentId attachmentId;
  final AttachmentsRepository attachmentsRepository;

  @override
  Future<AttachmentId> obtainKey(ImageConfiguration configuration) =>
      SynchronousFuture<AttachmentId>(attachmentId);

  @override
  ImageStreamCompleter loadImage(
    AttachmentId key,
    ImageDecoderCallback decode,
  ) => MultiFrameImageStreamCompleter(
    codec: _loadAsync(key, decode),
    scale: 1.0,
    debugLabel: 'AttachmentThumbnailProvider($attachmentId)',
    informationCollector: () => [
      DiagnosticsProperty<ImageProvider>('Image provider', this),
      DiagnosticsProperty<AttachmentId>('Image key', key),
    ],
  );

  Future<ui.Codec> _loadAsync(
    AttachmentId key,
    ImageDecoderCallback decode,
  ) async {
    final Uint8List? bytes;
    try {
      bytes = await attachmentsRepository.loadThumbnail(
        attachmentId: key,
        retryDownloadIfFailed: true,
      );
    } catch (_) {
      _evict(key);
      rethrow;
    }
    if (bytes == null) {
      _evict(key);
      throw StateError('No thumbnail for $key');
    }
    final buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
    return decode(buffer);
  }

  void _evict(AttachmentId key) =>
      scheduleMicrotask(() => PaintingBinding.instance.imageCache.evict(key));

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is AttachmentThumbnailProvider &&
          other.attachmentId == attachmentId;

  @override
  int get hashCode => attachmentId.hashCode;
}
