// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:flutter/widgets.dart';

/// A staged attachment in the composer: a picture, by the path of its file,
/// or a file, by its name.
@immutable
class MessageInputMediaItem {
  const MessageInputMediaItem({
    this.imagePath,
    required this.fileName,
    this.fileMeta,
  });

  /// Path of the picture. Null for a file, which stages as a chip.
  final String? imagePath;

  final String fileName;

  /// The chip's second line, "{EXT} • {size}".
  final String? fileMeta;
}

/// The strip of staged attachments shown above the field before sending.
class MessageInputMedia extends StatelessWidget {
  const MessageInputMedia({super.key, required this.items, this.onRemove});

  final List<MessageInputMediaItem> items;

  /// Unstages the item at the index.
  final void Function(int index)? onRemove;

  @override
  Widget build(BuildContext context) {
    final onRemove = this.onRemove;
    return Padding(
      padding: const EdgeInsets.only(
        top: MessageInputMediaTokens.gapAbove,
        bottom: MessageInputMediaTokens.gapBelow,
      ),
      // The scroll runs the full width of the chrome and the side inset lives
      // inside it, so items scroll out under the edge and are not cropped
      // short of it.
      child: SingleChildScrollView(
        scrollDirection: .horizontal,
        padding: const EdgeInsets.symmetric(
          horizontal: MessageInputMediaTokens.gapSide,
        ),
        child: Row(
          mainAxisSize: .min,
          spacing: MessageInputMediaTokens.gap,
          children: [
            for (final (index, item) in items.indexed)
              _MediaItem(
                item: item,
                onRemove: onRemove == null ? null : () => onRemove(index),
              ),
          ],
        ),
      ),
    );
  }
}

class _MediaItem extends StatelessWidget {
  const _MediaItem({required this.item, this.onRemove});

  final MessageInputMediaItem item;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final radius = BorderRadius.circular(MessageInputMediaTokens.radius);
    final imagePath = item.imagePath;
    final onRemove = this.onRemove;

    final content = imagePath != null
        ? ClipRRect(
            borderRadius: radius,
            child: SizedBox.square(
              dimension: MessageInputMediaTokens.thumbnailSize,
              child: _StillImage(
                path: imagePath,
                fallback: palette.fill.tertiary,
              ),
            ),
          )
        : _FileChip(item: item, radius: radius);

    return Stack(
      children: [
        content,
        if (onRemove != null)
          Positioned(
            top: MessageInputMediaTokens.removeOffset,
            right: MessageInputMediaTokens.removeOffset,
            child: ButtonIcon(
              variant: ButtonIconVariant.solid,
              icon: AppIconType.x,
              size: MessageInputMediaTokens.removeSize,
              iconSize: MessageInputMediaTokens.removeIconSize,
              fill: palette.backgroundElevated.primary,
              hitTargetSize: MessageInputMediaTokens.removeHitTarget,
              onPressed: onRemove,
            ),
          ),
      ],
    );
  }
}

/// A staged file: icon on the left, name over "{EXT} • {size}" on the right.
class _FileChip extends StatelessWidget {
  const _FileChip({required this.item, required this.radius});

  final MessageInputMediaItem item;
  final BorderRadius radius;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final meta = item.fileMeta;
    return Container(
      height: MessageInputMediaTokens.thumbnailSize,
      constraints: const BoxConstraints(
        minWidth: MessageInputMediaTokens.fileMinWidth,
        maxWidth: MessageInputMediaTokens.fileMaxWidth,
      ),
      padding: MessageInputMediaTokens.filePadding,
      decoration: BoxDecoration(
        color: palette.fill.tertiary,
        borderRadius: radius,
      ),
      child: Row(
        mainAxisSize: .min,
        children: [
          AppIcon.fileText(
            size: MessageInputMediaTokens.fileIconSize,
            color: palette.text.secondary,
          ),
          const SizedBox(width: MessageInputMediaTokens.fileIconGap),
          Flexible(
            child: Column(
              mainAxisSize: .min,
              mainAxisAlignment: .center,
              crossAxisAlignment: .start,
              children: [
                Text(
                  item.fileName,
                  maxLines: 1,
                  overflow: .ellipsis,
                  style: typeScale.body.s.style(
                    color: palette.text.primary,
                    weight: Weight.emphasized,
                  ),
                ),
                if (meta != null && meta.isNotEmpty) ...[
                  const SizedBox(height: MessageInputMediaTokens.fileTextGap),
                  Text(
                    meta,
                    maxLines: 1,
                    overflow: .ellipsis,
                    style: typeScale.body.mini.style(
                      color: palette.text.secondary,
                    ),
                  ),
                ],
              ],
            ),
          ),
          // Keeps the text clear of the remove button floating over the chip.
          const SizedBox(width: MessageInputMediaTokens.fileRemoveReserve),
        ],
      ),
    );
  }
}

/// The first frame of the picture at [path], decoded once at thumbnail size
/// and shown still, so a staged GIF does not play in the composer.
class _StillImage extends StatefulWidget {
  const _StillImage({required this.path, required this.fallback});

  final String path;

  /// Fill shown until the frame decodes, and if it fails to.
  final Color fallback;

  @override
  State<_StillImage> createState() => _StillImageState();
}

class _StillImageState extends State<_StillImage> {
  ui.Image? _frame;
  String? _decodedPath;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_decodedPath != widget.path) _decode();
  }

  @override
  void didUpdateWidget(_StillImage old) {
    super.didUpdateWidget(old);
    if (old.path != widget.path) _decode();
  }

  @override
  void dispose() {
    _frame?.dispose();
    super.dispose();
  }

  Future<void> _decode() async {
    final path = widget.path;
    _decodedPath = path;
    final side =
        MessageInputMediaTokens.thumbnailSize *
        MediaQuery.devicePixelRatioOf(context);
    try {
      final buffer = await ui.ImmutableBuffer.fromFilePath(path);
      final descriptor = await ui.ImageDescriptor.encoded(buffer);
      // The shorter side covers the thumbnail, a small picture is not
      // enlarged.
      final scale = math.min(
        1.0,
        side / math.min(descriptor.width, descriptor.height),
      );
      final codec = await descriptor.instantiateCodec(
        targetWidth: math.max(1, (descriptor.width * scale).round()),
        targetHeight: math.max(1, (descriptor.height * scale).round()),
      );
      final frame = await codec.getNextFrame();
      codec.dispose();
      descriptor.dispose();
      buffer.dispose();
      if (!mounted || path != widget.path) {
        frame.image.dispose();
        return;
      }
      setState(() {
        _frame?.dispose();
        _frame = frame.image;
      });
    } catch (_) {
      // Keeps the fallback fill.
    }
  }

  @override
  Widget build(BuildContext context) {
    final frame = _frame;
    if (frame == null) return ColoredBox(color: widget.fallback);
    return RawImage(image: frame, fit: .cover);
  }
}
