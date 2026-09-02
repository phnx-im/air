// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async' show unawaited;
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/fullscreen_image/fullscreen_image.dart';
import 'package:air/ds/patterns/fullscreen_image/fullscreen_image_tokens.dart';
import 'package:air/features/attachments/attachment_actions.dart';
import 'package:air/features/attachments/attachment_image_provider.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

String imageViewerHeroTag(UiAttachment attachment) =>
    'image-viewer-${attachment.attachmentId.uuid}';

Route<void> imageViewerRoute({
  required UiAttachment attachment,
  required UiImageMetadata metadata,
  required ImageProvider thumbnail,
}) {
  return PageRouteBuilder<void>(
    transitionDuration: const Duration(milliseconds: 280),
    reverseTransitionDuration: const Duration(milliseconds: 220),
    pageBuilder: (context, animation, secondaryAnimation) {
      return ImageViewer(
        attachment: attachment,
        metadata: metadata,
        thumbnail: thumbnail,
      );
    },
    transitionsBuilder: (context, animation, secondaryAnimation, child) {
      final fadeAnimation = animation.drive(
        CurveTween(curve: Curves.easeInOutCubicEmphasized),
      );
      return FadeTransition(opacity: fadeAnimation, child: child);
    },
  );
}

/// Hosts the fullscreen takeover for one attachment: it supplies the picture
/// and the actions, the takeover owns everything the viewer does with them.
class ImageViewer extends StatelessWidget {
  const ImageViewer({
    required this.attachment,
    required this.metadata,
    required this.thumbnail,
    super.key,
  });

  final UiAttachment attachment;
  final UiImageMetadata metadata;

  /// The decode the message is already painting. It opens the takeover, and it
  /// is what the hero flies, while the full-size decode is still on its way.
  final ImageProvider thumbnail;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: SemanticPalette.darkOf(context).function.neutral.black,
      body: FullscreenImage(
        tokens: FullscreenImageTokens.current,
        items: [
          FullscreenImageItem(
            image: AttachmentImageProvider(
              attachment: attachment,
              attachmentsRepository: RepositoryProvider.of(context),
            ),
            naturalSize: Size(
              metadata.width.toDouble(),
              metadata.height.toDouble(),
            ),
            placeholder: thumbnail,
            heroTag: imageViewerHeroTag(attachment),
          ),
        ],
        onClose: () => Navigator.pop(context),
        onShare: () => _share(context),
      ),
    );
  }

  /// The same split the message menu makes: iOS has no file store to save to,
  /// so it shares the picture on instead.
  void _share(BuildContext context) {
    if (Platform.isIOS) {
      unawaited(shareAttachments(context, [attachment]));
      return;
    }
    unawaited(saveAttachment(context, attachment));
  }
}
