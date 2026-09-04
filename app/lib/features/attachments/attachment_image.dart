// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/attachments/animated_attachment_image.dart';
import 'package:air/features/attachments/attachment_image_overlay.dart';
import 'package:air/features/attachments/attachment_thumbnail_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_blurhash/flutter_blurhash.dart';
import 'package:logging/logging.dart';

final _log = Logger('AttachmentImage');

/// Renders an attachment image loaded from the database.
///
/// Branches on [UiImageMetadata.isAnimated]: static images go through
/// [_StaticAttachmentImage], animated ones through [AnimatedAttachmentImage].
/// When the flag is not yet known (the attachment was not yet downloaded when
/// the message state was loaded), [_UnclassifiedAttachmentImage] classifies it
/// via the repository and then delegates to the right branch. The blurhash
/// stays underneath as the placeholder for all states.
///
/// [onTap] is forwarded to the static branch (image viewer); animated
/// attachments keep the tap for their own playback.
class AttachmentImage extends StatelessWidget {
  const AttachmentImage({
    super.key,
    required this.attachment,
    required this.imageMetadata,
    required this.fit,
    required this.isSender,
    this.onTap,
  });

  final UiAttachment attachment;
  final UiImageMetadata imageMetadata;
  final BoxFit fit;
  final bool isSender;

  /// Receives the provider the still picture is painted from. Unused on the
  /// animated branch, which keeps the tap for its own playback.
  final void Function(ImageProvider thumbnail)? onTap;

  @override
  Widget build(BuildContext context) {
    final content = switch (imageMetadata.isAnimated) {
      false => _StaticAttachmentImage(
        attachment: attachment,
        fit: fit,
        isSender: isSender,
        onTap: onTap,
      ),
      true => AnimatedAttachmentImage(
        attachment: attachment,
        fit: fit,
        isSender: isSender,
      ),
      null => _UnclassifiedAttachmentImage(
        attachment: attachment,
        fit: fit,
        isSender: isSender,
        onTap: onTap,
      ),
    };

    final blurhash = imageMetadata.blurhash;
    return AspectRatio(
      aspectRatio: imageMetadata.width / imageMetadata.height,
      child: Stack(
        fit: .expand,
        children: [
          // An own upload has no blurhash until it has been processed; the
          // thumbnail takes over shortly after.
          ColoredBox(color: SemanticPalette.of(context).fill.tertiary),
          if (blurhash != null) BlurHash(hash: blurhash),
          content,
        ],
      ),
    );
  }
}

/// An image attachment whose animated-vs-static classification is not yet in
/// the message state.
class _UnclassifiedAttachmentImage extends StatefulWidget {
  const _UnclassifiedAttachmentImage({
    required this.attachment,
    required this.fit,
    required this.isSender,
    this.onTap,
  });

  final UiAttachment attachment;
  final BoxFit fit;
  final bool isSender;
  final void Function(ImageProvider thumbnail)? onTap;

  @override
  State<_UnclassifiedAttachmentImage> createState() =>
      _UnclassifiedAttachmentImageState();
}

class _UnclassifiedAttachmentImageState
    extends State<_UnclassifiedAttachmentImage> {
  bool? _isAnimated;

  @override
  void initState() {
    super.initState();
    unawaited(_classify());
  }

  /// Classifies the attachment as animated or static, downloading it and
  /// backfilling the flag if needed.
  Future<void> _classify({bool retryDownloadIfFailed = false}) async {
    try {
      if (!mounted) return;
      final repository = context.read<AttachmentsRepository>();
      final isAnimated = await repository.isAttachmentAnimated(
        attachmentId: widget.attachment.attachmentId,
        retryDownloadIfFailed: retryDownloadIfFailed,
      );
      if (!mounted || isAnimated == null) return;
      setState(() => _isAnimated = isAnimated);
    } catch (e, st) {
      _log.severe('Failed to classify attachment', e, st);
    }
  }

  @override
  Widget build(BuildContext context) {
    return switch (_isAnimated) {
      false => _StaticAttachmentImage(
        attachment: widget.attachment,
        fit: widget.fit,
        isSender: widget.isSender,
        onTap: widget.onTap,
      ),
      true => AnimatedAttachmentImage(
        attachment: widget.attachment,
        fit: widget.fit,
        isSender: widget.isSender,
      ),
      null => AttachmentImageOverlay(
        attachmentId: widget.attachment.attachmentId,
        size: widget.attachment.size,
        isSender: widget.isSender,
        isAnimationPaused: false,
        onTapDownload: () => unawaited(_classify(retryDownloadIfFailed: true)),
      ),
    };
  }
}

/// Renders the still picture via [Image] + [AttachmentThumbnailProvider], so
/// frames live in Flutter's shared `imageCache`. Tap hands the provider
/// painted here to [onTap] (image viewer), so the viewer opens on a decode it
/// already has.
class _StaticAttachmentImage extends StatelessWidget {
  const _StaticAttachmentImage({
    required this.attachment,
    required this.fit,
    required this.isSender,
    this.onTap,
  });

  final UiAttachment attachment;
  final BoxFit fit;
  final bool isSender;
  final void Function(ImageProvider thumbnail)? onTap;

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: .expand,
      children: [
        // The decode is bounded by the box we lay out in, so the provider only
        // exists once that box is known -- and the tap has to hand that same
        // provider on.
        LayoutBuilder(
          builder: (context, constraints) {
            final thumbnail = _thumbnail(context, constraints);
            return GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => onTap?.call(thumbnail),
              child: Image(
                image: thumbnail,
                fit: fit,
                alignment: Alignment.center,
                errorBuilder: (_, _, _) => const SizedBox.shrink(),
              ),
            );
          },
        ),
        AttachmentImageOverlay(
          attachmentId: attachment.attachmentId,
          size: attachment.size,
          isSender: isSender,
          isAnimationPaused: false,
          // A known classification implies the content is on device, so a
          // download can never be offered here.
          onTapDownload: () {},
        ),
      ],
    );
  }

  /// The provider the still picture is painted from: a decode bounded by the
  /// box on screen, so the list holds thumbnails rather than full frames.
  ImageProvider _thumbnail(BuildContext context, BoxConstraints constraints) {
    final dpr = MediaQuery.devicePixelRatioOf(context);
    return ResizeImage(
      AttachmentThumbnailProvider(
        attachmentId: attachment.attachmentId,
        attachmentsRepository: context.read<AttachmentsRepository>(),
      ),
      // The box we lay out in comes from the sender-declared metadata, which
      // may not match the actual pixels. `exact` (the default) would decode to
      // the box like .fill and distort the image, so constrain the decode
      // instead of reshaping it.
      policy: .fit,
      width: constraints.maxWidth.isFinite
          ? (constraints.maxWidth * dpr).round()
          : null,
      height: constraints.maxHeight.isFinite
          ? (constraints.maxHeight * dpr).round()
          : null,
      allowUpscaling: false,
    );
  }
}
