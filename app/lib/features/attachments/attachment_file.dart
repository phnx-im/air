// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:air/util/scaffold_messenger.dart';
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
      mainAxisSize: .min,
      spacing: S.s16,
      children: [
        _AttachmentFileStatus(
          attachmentId: attachment.attachmentId,
          size: attachment.size,
          isSender: isSender,
          color: color,
        ),
        // Flexible is needed to make the text wrap if the filename is too long
        Flexible(
          fit: .loose,
          child: Column(
            crossAxisAlignment: .start,
            children: [
              Text(
                attachment.filename,
                style: typeScale.body.regular.style(color: color),
              ),
              Text(
                loc.bytesToHumanReadable(attachment.size),
                style: typeScale.body.xs.style(color: color),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _AttachmentFileStatus extends HookWidget {
  const _AttachmentFileStatus({
    required this.attachmentId,
    required this.size,
    required this.isSender,
    required this.color,
  });

  final AttachmentId attachmentId;
  final int size;
  final bool isSender;

  final Color color;

  @override
  Widget build(BuildContext context) {
    final retries = useState(0); // bump to force stream re-subscription
    final statusStream = useMemoized(
      () => context.read<AttachmentsRepository>().statusStream(
        attachmentId: attachmentId,
      ),
      [attachmentId, retries.value],
    );
    final status = useStream<UiAttachmentStatus>(statusStream);
    final palette = SemanticPalette.of(context);

    return Center(
      child: switch (status.data) {
        null || UiAttachmentStatus_Completed() => AppIcon.paperclip(
          size: 32,
          color: color,
        ),
        UiAttachmentStatus_NotFound() => ButtonIcon(
          variant: ButtonIconVariant.plain,
          icon: AppIconType.circleAlert,
          size: ButtonIconSize.s48,
          iconSize: S.s32,
          iconColor: color,
          onPressed: () {
            showSnackBarStandalone(
              (loc) => SnackBar(content: Text(loc.attachment_notFound)),
              tone: SnackbarTone.danger,
            );
          },
        ),
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() when isSender => ButtonIcon(
          variant: ButtonIconVariant.solid,
          icon: AppIconType.upload,
          size: ButtonIconSize.s48,
          iconSize: S.s32,
          iconColor: palette.text.secondary,
          fill: palette.backgroundBase.tertiary,
          onPressed: () {
            retries.value++;
            context.read<ChatDetailsCubit>().retryUploadAttachment(
              attachmentId,
            );
          },
        ),
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() => ButtonIcon(
          variant: ButtonIconVariant.solid,
          icon: AppIconType.download,
          size: ButtonIconSize.s48,
          iconSize: S.s32,
          iconColor: palette.text.secondary,
          fill: palette.backgroundBase.tertiary,
          onPressed: () {
            retries.value++;
            context.read<AttachmentsRepository>().loadAttachment(
              attachmentId: attachmentId,
            );
          },
        ),
        UiAttachmentStatus_Progress(field0: final loaded) => Stack(
          alignment: Alignment.center,
          children: [
            CircularProgressIndicator(
              strokeWidth: StrokeWidth.px2,
              backgroundColor: color.withValues(alpha: Alpha.a10),
              valueColor: AlwaysStoppedAnimation<Color>(color),
              value: loaded / BigInt.from(size),
            ),
            ButtonIcon(
              variant: ButtonIconVariant.plain,
              icon: AppIconType.x,
              iconSize: S.s24,
              iconColor: color,
              hitTargetSize: S.s48,
              onPressed: () {
                context.read<AttachmentsRepository>().cancel(
                  attachmentId: attachmentId,
                );
              },
            ),
          ],
        ),
      },
    );
  }
}
