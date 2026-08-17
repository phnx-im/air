// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/painting.dart';
import 'package:air/core/core.dart';

/// Same as [MemoryImage] but caches the result in memory under the given [tag]
///
/// If [targetWidth] and/or [targetHeight] are provided, the image is resized to
/// those dimensions while decoding (before caching).
class CachedMemoryImage extends ImageProvider<CachedMemoryImage> {
  const CachedMemoryImage(
    this.tag,
    this.bytes, {
    this.targetWidth,
    this.targetHeight,
  });

  factory CachedMemoryImage.fromImageData(
    ImageData imageData, {
    int? targetWidth,
    int? targetHeight,
  }) => CachedMemoryImage(
    imageData.hash,
    imageData.data,
    targetWidth: targetWidth,
    targetHeight: targetHeight,
  );

  final String tag;
  final Uint8List bytes;
  final int? targetWidth;
  final int? targetHeight;

  @override
  ImageStreamCompleter loadImage(
    CachedMemoryImage key,
    ImageDecoderCallback decode,
  ) {
    return MultiFrameImageStreamCompleter(
      codec: _loadAsync(key, decode: decode),
      scale: 1.0,
      debugLabel: 'CachedMemoryImage($tag)',
    );
  }

  Future<ui.Codec> _loadAsync(
    CachedMemoryImage key, {
    required ImageDecoderCallback decode,
  }) async {
    final buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
    if (targetWidth == null && targetHeight == null) {
      return decode(buffer);
    }
    return decode(
      buffer,
      getTargetSize: (intrinsicWidth, intrinsicHeight) {
        final widthScale = (targetWidth != null && intrinsicWidth > 0)
            ? targetWidth! / intrinsicWidth
            : null;
        final heightScale = (targetHeight != null && intrinsicHeight > 0)
            ? targetHeight! / intrinsicHeight
            : null;
        final double scale;
        if (widthScale != null && heightScale != null) {
          scale = math.max(widthScale, heightScale);
        } else {
          scale = widthScale ?? heightScale!;
        }
        final clampedScale = math.min(scale, 1.0);
        return ui.TargetImageSize(
          width: (intrinsicWidth * clampedScale).round(),
          height: (intrinsicHeight * clampedScale).round(),
        );
      },
    );
  }

  @override
  Future<CachedMemoryImage> obtainKey(ImageConfiguration configuration) {
    return SynchronousFuture<CachedMemoryImage>(this);
  }

  @override
  bool operator ==(Object other) =>
      other.runtimeType == runtimeType &&
      other is CachedMemoryImage &&
      other.tag == tag &&
      other.targetWidth == targetWidth &&
      other.targetHeight == targetHeight;

  @override
  int get hashCode => Object.hash(tag, targetWidth, targetHeight);

  @override
  String toString() =>
      '${objectRuntimeType(this, 'CachedMemoryImage')}($tag, '
      'targetWidth: $targetWidth, targetHeight: $targetHeight)';
}
