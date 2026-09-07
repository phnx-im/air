// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:ui' as ui;

import 'package:air/core/core.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// Transfer status overlay for an image attachment: upload/download retry,
/// progress with cancel, not-found, and the paused-animation indicator.
class AttachmentImageOverlay extends HookWidget {
  const AttachmentImageOverlay({
    super.key,
    required this.attachmentId,
    required this.size,
    required this.isSender,
    required this.isAnimationPaused,
    required this.onTapDownload,
  });

  final AttachmentId attachmentId;
  final int size;
  final bool isSender;
  final bool isAnimationPaused;

  final VoidCallback onTapDownload;

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

    return Align(
      alignment: Alignment.center,
      child: switch (status.data) {
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() when isSender => _BlurredPill(
          child: ButtonIcon(
            variant: ButtonIconVariant.plain,
            icon: AppIconType.upload,
            iconSize: S.s24,
            hitTargetSize: S.s48,
            onPressed: () {
              retries.value++;
              context.read<ChatDetailsCubit>().retryUploadAttachment(
                attachmentId,
              );
            },
          ),
        ),
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() => _BlurredPill(
          child: ButtonIcon(
            variant: ButtonIconVariant.plain,
            icon: AppIconType.download,
            iconSize: S.s24,
            hitTargetSize: S.s48,
            onPressed: () {
              retries.value++;
              onTapDownload();
            },
          ),
        ),
        UiAttachmentStatus_Progress(field0: final loaded) => _BlurredPill(
          child: Stack(
            alignment: Alignment.center,
            children: [
              CircularProgressIndicator(
                strokeWidth: StrokeWidth.px2,
                valueColor: AlwaysStoppedAnimation<Color>(palette.text.primary),
                backgroundColor: Colors.transparent,
                value: loaded / BigInt.from(size),
              ),
              ButtonIcon(
                variant: ButtonIconVariant.plain,
                icon: AppIconType.x,
                iconSize: S.s24,
                hitTargetSize: S.s48,
                onPressed: () {
                  context.read<AttachmentsRepository>().cancel(
                    attachmentId: attachmentId,
                  );
                },
              ),
            ],
          ),
        ),
        UiAttachmentStatus_NotFound() => ButtonIcon(
          variant: ButtonIconVariant.plain,
          icon: AppIconType.circleAlert,
          size: ButtonIconSize.s48,
          iconSize: S.s32,
          iconColor: palette.text.primary,
          onPressed: () {
            showSnackBarStandalone(
              (loc) => SnackBar(content: Text(loc.attachment_notFound)),
              tone: SnackbarTone.danger,
            );
          },
        ),
        UiAttachmentStatus_Completed() when isAnimationPaused => IgnorePointer(
          child: _BlurredPill(
            child: AppIcon.rotateCw(size: 24, color: palette.text.primary),
          ),
        ),
        null || UiAttachmentStatus_Completed() => const SizedBox.shrink(),
      },
    );
  }
}

/// Centered blurred pill used as the visual container for image overlays.
class _BlurredPill extends StatelessWidget {
  const _BlurredPill({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(CornerRadius.full),
      child: BackdropFilter(
        filter: ui.ImageFilter.blur(
          sigmaX: Effect.blur(BlurLevel.medium),
          sigmaY: Effect.blur(BlurLevel.medium),
        ),
        child: ColoredBox(
          color: SemanticPalette.of(context).backgroundMaterial.tertiary,
          child: Padding(padding: const EdgeInsets.all(S.s16), child: child),
        ),
      ),
    );
  }
}
