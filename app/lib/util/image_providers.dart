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
class TaggedMemoryImage extends ImageProvider<TaggedMemoryImage> {
  const TaggedMemoryImage(
    this.tag,
    this.bytes, {
    this.targetWidth,
    this.targetHeight,
  });

  factory TaggedMemoryImage.fromImageData(
    ImageData imageData, {
    int? targetWidth,
    int? targetHeight,
  }) => TaggedMemoryImage(
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
    TaggedMemoryImage key,
    ImageDecoderCallback decode,
  ) {
    return MultiFrameImageStreamCompleter(
      codec: _loadAsync(key, decode: decode),
      scale: 1.0,
      debugLabel: 'CachedMemoryImage($tag)',
    );
  }

  Future<ui.Codec> _loadAsync(
    TaggedMemoryImage key, {
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
  Future<TaggedMemoryImage> obtainKey(ImageConfiguration configuration) {
    return SynchronousFuture<TaggedMemoryImage>(this);
  }

  @override
  bool operator ==(Object other) =>
      other.runtimeType == runtimeType &&
      other is TaggedMemoryImage &&
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

/// Wraps a provider so its decode never enters [PaintingBinding.imageCache].
///
/// A full-size decode is tens of MB and would flush every thumbnail out of
/// the shared cache. This holds the one decode instead, until [dispose].
class KeepAliveImage<T extends Object> extends ImageProvider<T> {
  KeepAliveImage(this.inner);

  final ImageProvider<T> inner;

  ImageStreamCompleter? _completer;
  ImageStreamCompleterHandle? _handle;

  @override
  Future<T> obtainKey(ImageConfiguration configuration) =>
      inner.obtainKey(configuration);

  @override
  ImageStreamCompleter loadImage(T key, ImageDecoderCallback decode) =>
      inner.loadImage(key, decode);

  @override
  void resolveStreamForKey(
    ImageConfiguration configuration,
    ImageStream stream,
    T key,
    ImageErrorListener handleError,
  ) {
    if (stream.completer != null) return;
    final completer = _completer ??= loadImage(
      key,
      PaintingBinding.instance.instantiateImageCodecWithSize,
    );
    // The cache normally holds this. Without it the completer disposes itself
    // as soon as the view stops listening.
    _handle ??= completer.keepAlive();
    stream.setCompleter(completer);
  }

  void dispose() {
    _handle?.dispose();
    _handle = null;
    _completer = null;
  }

  @override
  bool operator ==(Object other) =>
      other is KeepAliveImage<T> && other.inner == inner;

  @override
  int get hashCode => inner.hashCode;
}
