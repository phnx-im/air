// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:ui';

import 'package:air/chat/chat_details_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_blurhash/flutter_blurhash.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
import 'package:logging/logging.dart';
import 'package:air/core/core.dart';
import 'package:air/ui/colors/themes.dart';

import 'attachment_image_provider.dart';

final _log = Logger('AttachmentImage');

/// An image that is loaded from the database via an [AttachmentsRepository].
///
/// During loading, image's blurhash is shown instead of the image.
class AttachmentImage extends StatelessWidget {
  const AttachmentImage({
    super.key,
    required this.attachment,
    required this.imageMetadata,
    required this.fit,
  });

  final UiAttachment attachment;
  final UiImageMetadata imageMetadata;
  final BoxFit fit;

  @override
  Widget build(BuildContext context) {
    return AspectRatio(
      aspectRatio: imageMetadata.width / imageMetadata.height,
      child: Stack(
        fit: StackFit.expand,
        children: [
          BlurHash(hash: imageMetadata.blurhash),
          Image(
            image: AttachmentImageProvider(
              attachment: attachment,
              attachmentsRepository: RepositoryProvider.of(context),
            ),
            loadingBuilder: loadingBuilder,
            fit: fit,
            alignment: Alignment.center,
            errorBuilder: (context, error, stackTrace) {
              _log.severe('Failed to load attachment: $error');
              return Align(
                child: iconoir.WarningCircle(
                  width: 32,
                  height: 32,
                  color: CustomColorScheme.of(context).text.primary,
                ),
              );
            },
          ),
          _UploadStatus(
            attachmentId: attachment.attachmentId,
            size: attachment.size,
          ),
        ],
      ),
    );
  }

  Widget loadingBuilder(
    BuildContext context,
    Widget child,
    ImageChunkEvent? loadingProgress,
  ) {
    if (loadingProgress == null) {
      return child;
    }
    return Center(
      child: CircularProgressIndicator(
        valueColor: AlwaysStoppedAnimation<Color>(
          CustomColorScheme.of(context).backgroundBase.tertiary,
        ),
        backgroundColor: Colors.transparent,
        value: loadingProgress.expectedTotalBytes != null
            ? loadingProgress.cumulativeBytesLoaded /
                  loadingProgress.expectedTotalBytes!
            : null,
      ),
    );
  }
}

class _UploadStatus extends HookWidget {
  const _UploadStatus({required this.attachmentId, required this.size});

  final AttachmentId attachmentId;
  final int size;

  @override
  Widget build(BuildContext context) {
    final uploadStatusSteam = useMemoized(
      () => context.read<AttachmentsRepository>().statusStream(
        attachmentId: attachmentId,
      ),
      [attachmentId],
    );
    final uploadStatus = useStream<UiAttachmentStatus>(uploadStatusSteam);

    return Align(
      alignment: Alignment.center,
      child: switch (uploadStatus.data) {
        null || UiAttachmentStatus_Completed() => const SizedBox.shrink(),
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() => OutlinedButton(
          onPressed: () {
            context.read<ChatDetailsCubit>().retryUploadAttachment(
              attachmentId,
            );
          },
          child: Row(
            mainAxisAlignment: .center,
            mainAxisSize: MainAxisSize.min,
            children: [
              iconoir.Upload(
                width: 32,
                height: 32,
                color: CustomColorScheme.of(context).text.primary,
              ),
              const SizedBox(width: Spacings.xxxs),
              Text(
                "Try again",
                style: TextStyle(
                  color: CustomColorScheme.of(context).text.primary,
                  fontSize: LabelFontSize.base.size,
                ),
              ),
            ],
          ),
        ),
        UiAttachmentStatus_Progress(field0: final loaded) => ClipRRect(
          borderRadius: BorderRadius.circular(100),
          child: BackdropFilter(
            filter: ImageFilter.blur(sigmaX: 10, sigmaY: 10),
            child: Padding(
              padding: const EdgeInsets.all(Spacings.xs),
              child: Stack(
                alignment: Alignment.center,
                children: [
                  CircularProgressIndicator(
                    strokeWidth: 2,
                    valueColor: AlwaysStoppedAnimation<Color>(
                      CustomColorScheme.of(context).text.primary,
                    ),
                    backgroundColor: Colors.transparent,
                    value: loaded / BigInt.from(size),
                  ),
                  IconButton(
                    onPressed: () {
                      context.read<AttachmentsRepository>().cancel(
                        attachmentId: attachmentId,
                      );
                    },
                    icon: iconoir.Xmark(
                      color: CustomColorScheme.of(context).text.primary,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      },
    );
  }
}
