// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:ui' as ui;

import 'package:air/core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// Paints a picture it decodes and owns, outside Flutter's `imageCache`.
///
/// Every [ImageProvider] goes through `imageCache.putIfAbsent`, which suits a
/// picture painted in several places and reused. It does not suit a big one
/// looked at once, which would then evict the good entries from the cache.
class OwnedPicture extends HookWidget {
  /// The image file at [path] (i.e. staged for an upload) scaled down.
  OwnedPicture.file(
    String path, {
    super.key,
    this.loading = const SizedBox.shrink(),
    this.error,
    this.builder,
    this.fit = BoxFit.contain,
    this.filterQuality = FilterQuality.medium,
  }) : _decode = (() => _decodeFile(path)),
       _keys = [path];

  /// A stored attachment, at the full fidelity.
  OwnedPicture.attachment({
    required AttachmentId attachmentId,
    required AttachmentsRepository repository,
    super.key,
    this.loading = const SizedBox.shrink(),
    this.error,
    this.builder,
    this.fit = BoxFit.contain,
    this.filterQuality = FilterQuality.medium,
  }) : _decode = (() => _decodeAttachment(repository, attachmentId)),
       _keys = [repository, attachmentId];

  final Future<ui.Image> Function() _decode;
  final List<Object?> _keys;

  /// Displayed until the picture arrives.
  final Widget loading;

  /// Replaces the picture when the decode fails.
  final Widget? error;

  /// Wraps the picture once it is there, for a frame that has to be laid out
  /// from the size it decoded to. Defaults to the picture on its own.
  final Widget Function(BuildContext context, Size size, Widget picture)?
  builder;

  final BoxFit fit;
  final FilterQuality filterQuality;

  @override
  Widget build(BuildContext context) {
    final decoded = useMemoized(_decode, _keys);
    useEffect(
      () =>
          () => decoded.then((image) => image.dispose()).ignore(),
      [decoded],
    );
    final image = useFuture(decoded);

    if (image.hasError) {
      return error ?? loading;
    }

    final decodedImage = image.data;
    if (decodedImage == null) {
      return loading;
    }

    final picture = RawImage(
      image: decodedImage.clone(),
      fit: fit,
      filterQuality: filterQuality,
    );

    if (builder == null) {
      return picture;
    }

    return builder!(
      context,
      Size(decodedImage.width.toDouble(), decodedImage.height.toDouble()),
      picture,
    );
  }
}

Future<ui.Image> _decodeFile(String path) async {
  // The codec disposes the buffer.
  final buffer = await ui.ImmutableBuffer.fromFilePath(path);
  return _frame(
    await ui.instantiateImageCodecWithSize(buffer, getTargetSize: _displayCap),
  );
}

Future<ui.Image> _decodeAttachment(
  AttachmentsRepository repository,
  AttachmentId attachmentId,
) async {
  final bytes = await repository.loadImageAttachment(
    attachmentId: attachmentId,
    retryDownloadIfFailed: false,
  );
  final buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
  return _frame(await ui.instantiateImageCodecWithSize(buffer));
}

Future<ui.Image> _frame(ui.Codec codec) async {
  try {
    return (await codec.getNextFrame()).image;
  } finally {
    codec.dispose();
  }
}

/// Caps a decode at twice the display's longest side, past what a pinch zoom
/// resolves. Only an outsized picture is scaled down at all, and that is the
/// one worth keeping off the heap at full size.
ui.TargetImageSize _displayCap(int width, int height) {
  final display = ui.PlatformDispatcher.instance.implicitView?.physicalSize;
  if (display == null) {
    return const ui.TargetImageSize();
  }
  final bound =
      2 *
      (display.width > display.height ? display.width : display.height).round();
  if (width <= bound && height <= bound) {
    return const ui.TargetImageSize();
  }
  return width >= height
      ? ui.TargetImageSize(width: bound)
      : ui.TargetImageSize(height: bound);
}
