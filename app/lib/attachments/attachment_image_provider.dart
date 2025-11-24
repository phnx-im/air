// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/painting.dart';
import 'package:air/core/core.dart';
import 'dart:ui' as ui;

/// Loads an attachment image from the database via an [AttachmentsRepository].
class AttachmentImageProvider extends ImageProvider<UiAttachment> {
  const AttachmentImageProvider({
    required this.attachment,
    required this.attachmentsRepository,
  });

  final UiAttachment attachment;
  final AttachmentsRepository attachmentsRepository;

  @override
  Future<UiAttachment> obtainKey(ImageConfiguration configuration) {
    return SynchronousFuture<UiAttachment>(attachment);
  }

  @override
  ImageStreamCompleter loadImage(
    UiAttachment key,
    ImageDecoderCallback decode,
  ) {
    final chunkEvents = StreamController<ImageChunkEvent>();
    return MultiFrameImageStreamCompleter(
      codec: _loadAsync(key, decode, chunkEvents),
      chunkEvents: chunkEvents.stream,
      scale: 1.0,
      debugLabel: "AttachmentImageProvider(${attachment.attachmentId})",
      informationCollector: () => <DiagnosticsNode>[
        DiagnosticsProperty<ImageProvider>('Image provider', this),
        DiagnosticsProperty<UiAttachment>('Image key', key),
      ],
    );
  }

  Future<ui.Codec> _loadAsync(
    final UiAttachment key,
    final ImageDecoderCallback decode,
    final StreamController<ImageChunkEvent> chunkEvents,
  ) async {
    Uint8List bytes;
    try {
      bytes = await attachmentsRepository.loadImageAttachment(
        attachmentId: key.attachmentId,
        chunkEventCallback: (cumulativeBytesLoaded) {
          chunkEvents.add(
            ImageChunkEvent(
              cumulativeBytesLoaded: cumulativeBytesLoaded.toInt(),
              expectedTotalBytes: key.size,
            ),
          );
        },
      );
    } catch (e) {
      scheduleMicrotask(() {
        PaintingBinding.instance.imageCache.evict(key);
      });
      rethrow;
    } finally {
      chunkEvents.close();
    }

    final buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
    return decode(buffer);
  }

  @override
  bool operator ==(Object other) {
    if (other.runtimeType != runtimeType) {
      return false;
    }
    return other is AttachmentImageProvider && other.attachment == attachment;
  }

  @override
  int get hashCode => attachment.hashCode;

  @override
  String toString() =>
      '${objectRuntimeType(this, "AttachmentImageProvider")}(${attachment.attachmentId})';
}
