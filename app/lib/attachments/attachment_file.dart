// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/ui/icons/app_icon.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

class AttachmentFile extends HookWidget {
  const AttachmentFile({
    super.key,
    required this.attachment,
    required this.isSender,
    required this.color,
  });

  final UiAttachment attachment;
  final bool isSender;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Row(
      mainAxisSize: MainAxisSize.min,
      spacing: Spacings.s,
      children: [
        isSender
            ? _UploadStatus(
                attachmentId: attachment.attachmentId,
                size: attachment.size,
                color: color,
              )
            : AppIcon(
                type: AppIconType.attachment,
                size: 32,
                color: color,
              ),
        Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              attachment.filename,
              style: TextStyle(fontSize: BodyFontSize.base.size, color: color),
            ),
            Text(
              loc.bytesToHumanReadable(attachment.size),
              style: TextStyle(
                fontSize: BodyFontSize.small2.size,
                color: color,
              ),
            ),
          ],
        ),
      ],
    );
  }
}

class _UploadStatus extends HookWidget {
  const _UploadStatus({
    required this.attachmentId,
    required this.size,
    required this.color,
  });

  final AttachmentId attachmentId;
  final int size;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final uploadStatusSteam = useMemoized(
      () => context.read<AttachmentsRepository>().statusStream(
        attachmentId: attachmentId,
      ),
      [attachmentId],
    );
    final uploadStatus = useStream<UiAttachmentStatus>(uploadStatusSteam);

    return Center(
      child: switch (uploadStatus.data) {
        null || UiAttachmentStatus_Completed() => AppIcon(
            type: AppIconType.attachment,
            size: 32,
            color: color,
          ),
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() => IconButton(
          onPressed: () {
            context.read<ChatDetailsCubit>().retryUploadAttachment(
              attachmentId,
            );
          },
          style: IconButton.styleFrom(
            backgroundColor: CustomColorScheme.of(
              context,
            ).backgroundBase.tertiary,
          ),
          icon: AppIcon(
              type: AppIconType.upload,
              size: 32,
              color: CustomColorScheme.of(context).text.secondary,
            ),
        ),
        UiAttachmentStatus_Progress(field0: final loaded) => Stack(
          alignment: Alignment.center,
          children: [
            CircularProgressIndicator(
              strokeWidth: 2,
              backgroundColor: color.withValues(alpha: 0.1),
              valueColor: AlwaysStoppedAnimation<Color>(color),
              value: loaded / BigInt.from(size),
            ),
            IconButton(
              onPressed: () {
                context.read<AttachmentsRepository>().cancel(
                  attachmentId: attachmentId,
                );
              },
              icon: AppIcon(
                  type: AppIconType.close,
                  size: 32,
                  color: color,
                ),
            ),
          ],
        ),
      },
    );
  }
}
