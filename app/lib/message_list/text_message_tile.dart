// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/api/markdown.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/attachments/attachments.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/timestamp.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/ui/typography/monospace.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:iconoir_flutter/regular/attachment.dart';
import 'package:iconoir_flutter/regular/xmark.dart';

import 'message_renderer.dart';

const double largeCornerRadius = Spacings.sm;
const double smallCornerRadius = Spacings.xxs;
const double messageHorizontalPadding = Spacings.s;
const double messageVerticalPadding = Spacings.xxs;

const _messagePadding = EdgeInsets.symmetric(
  horizontal: messageHorizontalPadding,
  vertical: messageVerticalPadding,
);

class TextMessageTile extends StatelessWidget {
  const TextMessageTile({
    required this.messageId,
    required this.contentMessage,
    required this.timestamp,
    required this.flightPosition,
    required this.status,
    required this.isSender,
    required this.showSender,
    super.key,
  });

  final MessageId messageId;
  final UiContentMessage contentMessage;
  final String timestamp;
  final UiFlightPosition flightPosition;
  final UiMessageStatus status;
  final bool isSender;
  final bool showSender;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        if (showSender && !isSender && flightPosition.isFirst)
          _Sender(sender: contentMessage.sender, isSender: false),
        _MessageView(
          messageId: messageId,
          contentMessage: contentMessage,
          timestamp: timestamp,
          isSender: isSender,
          flightPosition: flightPosition,
          status: status,
        ),
      ],
    );
  }
}

class _MessageView extends HookWidget {
  const _MessageView({
    required this.messageId,
    required this.contentMessage,
    required this.timestamp,
    required this.flightPosition,
    required this.isSender,
    required this.status,
  });

  final MessageId messageId;
  final UiContentMessage contentMessage;
  final String timestamp;
  final UiFlightPosition flightPosition;
  final bool isSender;
  final UiMessageStatus status;

  @override
  Widget build(BuildContext context) {
    final isRevealed = useState(false);

    final showMessageStatus =
        isSender &&
        flightPosition.isLast &&
        status != UiMessageStatus.sending &&
        status != UiMessageStatus.hidden;

    return Align(
      alignment: isSender ? Alignment.centerRight : Alignment.centerLeft,
      child: LayoutBuilder(
        builder: (context, constraints) {
          return ConstrainedBox(
            constraints: BoxConstraints(maxWidth: constraints.maxWidth * 5 / 6),
            child: Container(
              padding: EdgeInsets.only(
                top: flightPosition.isFirst ? 5 : 0,
                bottom: flightPosition.isLast ? 5 : 0,
              ),
              child: Column(
                crossAxisAlignment:
                    isSender
                        ? CrossAxisAlignment.end
                        : CrossAxisAlignment.start,
                children: [
                  InkWell(
                    mouseCursor: SystemMouseCursors.basic,
                    onTap: () => isRevealed.value = true,
                    onLongPress:
                        isSender
                            ? () => context
                                .read<ChatDetailsCubit>()
                                .editMessage(messageId: messageId)
                            : null,
                    child: _MessageContent(
                      content: contentMessage.content,
                      isSender: isSender,
                      flightPosition: flightPosition,
                      isEdited: contentMessage.edited,
                      isHidden:
                          status == UiMessageStatus.hidden && !isRevealed.value,
                    ),
                  ),
                  if (flightPosition.isLast) ...[
                    const SizedBox(height: 2),
                    Row(
                      mainAxisAlignment:
                          isSender
                              ? MainAxisAlignment.end
                              : MainAxisAlignment.start,
                      children: [
                        const SizedBox(width: Spacings.s),
                        Timestamp(timestamp),
                        if (showMessageStatus)
                          const SizedBox(width: Spacings.xxxs),
                        if (showMessageStatus)
                          DoubleCheckIcon(
                            size: LabelFontSize.small2.size,
                            singleCheckIcon: status == UiMessageStatus.sent,
                            inverted: status == UiMessageStatus.read,
                          ),
                        const SizedBox(width: Spacings.xs),
                      ],
                    ),
                  ],
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

class _MessageContent extends StatelessWidget {
  const _MessageContent({
    required this.content,
    required this.isSender,
    required this.flightPosition,
    required this.isEdited,
    required this.isHidden,
  });

  final UiMimiContent content;
  final bool isSender;
  final UiFlightPosition flightPosition;
  final bool isEdited;
  final bool isHidden;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final bool isDeleted = content.replaces != null && content.content == null;

    final contentElements =
        isHidden
            ? [
              Padding(
                padding: _messagePadding,
                child: Text(
                  loc.textMessage_hiddenPlaceholder,
                  style: TextStyle(
                    fontStyle: FontStyle.italic,
                    fontSize: BodyFontSize.base.size,
                    color: CustomColorScheme.of(context).text.tertiary,
                  ),
                ),
              ),
            ]
            : [
              if (isDeleted)
                Padding(
                  padding: _messagePadding,
                  child: buildBlockElement(
                    context,
                    BlockElement.error(loc.textMessage_deleted),
                    isSender,
                  ),
                ),
              if (content.attachments.firstOrNull case final attachment?)
                switch (attachment.imageMetadata) {
                  null => _FileAttachmentContent(
                    attachment: attachment,
                    isSender: isSender,
                  ),
                  final imageMetadata => _ImageAttachmentContent(
                    attachment: attachment,
                    imageMetadata: imageMetadata,
                    isSender: isSender,
                    flightPosition: flightPosition,
                    hasMessage: content.content?.elements.isNotEmpty ?? false,
                  ),
                },
              ...(content.content?.elements ?? []).map(
                (inner) => Padding(
                  padding: _messagePadding.copyWith(
                    bottom: isEdited ? 0 : null,
                  ),
                  child: buildBlockElement(context, inner.element, isSender),
                ),
              ),
              // The edited label is no longer included here
            ];

    return Padding(
      padding: const EdgeInsets.only(bottom: 1.5),
      child: Container(
        alignment:
            isSender
                ? AlignmentDirectional.topEnd
                : AlignmentDirectional.topStart,
        // There's a bug in the linter
        // ignore: avoid_unnecessary_containers
        child: Container(
          decoration: BoxDecoration(
            borderRadius: _messageBorderRadius(isSender, flightPosition),
            color:
                isSender
                    ? CustomColorScheme.of(context).message.selfBackground
                    : CustomColorScheme.of(context).message.otherBackground,
          ),
          child: DefaultTextStyle.merge(
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                // Main content (reserves space if edited)
                Column(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: contentElements,
                    ),
                    if (!isDeleted && isEdited)
                      Padding(
                        padding: const EdgeInsets.only(
                          left: Spacings.s,
                          right: Spacings.s,
                          bottom: Spacings.xxs,
                        ),
                        child: SelectionContainer.disabled(
                          child: Text(
                            loc.textMessage_edited,
                            style: Theme.of(
                              context,
                            ).textTheme.bodySmall!.copyWith(
                              color:
                                  isSender
                                      ? CustomColorScheme.of(
                                        context,
                                      ).message.selfEditedLabel
                                      : CustomColorScheme.of(
                                        context,
                                      ).message.otherEditedLabel,
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Sender extends StatelessWidget {
  const _Sender({required this.sender, required this.isSender});

  final UiUserId sender;
  final bool isSender;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: sender),
    );

    return Padding(
      padding: const EdgeInsets.only(top: Spacings.xs, bottom: Spacings.xxs),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          UserAvatar(
            displayName: profile.displayName,
            image: profile.profilePicture,
            size: Spacings.m,
          ),
          const SizedBox(width: Spacings.xs),
          _DisplayName(displayName: profile.displayName, isSender: isSender),
        ],
      ),
    );
  }
}

class _DisplayName extends StatelessWidget {
  const _DisplayName({required this.displayName, required this.isSender});

  final String displayName;
  final bool isSender;

  @override
  Widget build(BuildContext context) {
    final text = isSender ? "You" : displayName;
    final textUpper = text.toUpperCase();
    return SelectionContainer.disabled(
      child: Text(
        textUpper,
        style: TextStyle(
          color: CustomColorScheme.of(context).text.tertiary,
          fontSize: LabelFontSize.small2.size,
          fontWeight: FontWeight.w100,
          fontFamily: getSystemMonospaceFontFamily(),
          letterSpacing: 1,
        ),
        overflow: TextOverflow.ellipsis,
      ),
    );
  }
}

class _FileAttachmentContent extends StatelessWidget {
  const _FileAttachmentContent({
    required this.attachment,
    required this.isSender,
  });

  final UiAttachment attachment;
  final bool isSender;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Padding(
      padding: _messagePadding,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        spacing: Spacings.s,
        children: [
          Attachment(
            width: 32,
            color:
                isSender
                    ? CustomColorScheme.of(context).message.selfText
                    : CustomColorScheme.of(context).message.otherText,
          ),
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                attachment.filename,
                style: TextStyle(
                  fontSize: BodyFontSize.base.size,
                  color:
                      isSender
                          ? CustomColorScheme.of(context).message.selfText
                          : CustomColorScheme.of(context).message.otherText,
                ),
              ),
              Text(
                loc.bytesToHumanReadable(attachment.size),
                style: TextStyle(
                  fontSize: BodyFontSize.small2.size,
                  color:
                      isSender
                          ? CustomColorScheme.of(context).message.selfText
                          : CustomColorScheme.of(context).message.otherText,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _ImageAttachmentContent extends StatelessWidget {
  _ImageAttachmentContent({
    required this.attachment,
    required this.imageMetadata,
    required this.isSender,
    required this.flightPosition,
    required this.hasMessage,
  });

  final UiAttachment attachment;
  final UiImageMetadata imageMetadata;
  final bool isSender;
  final UiFlightPosition flightPosition;
  final bool hasMessage;

  final overlayController = OverlayPortalController();

  @override
  Widget build(BuildContext context) {
    return OverlayPortal(
      controller: overlayController,
      overlayChildBuilder:
          (BuildContext context) => _ImagePreview(
            attachment: attachment,
            imageMetadata: imageMetadata,
            isSender: isSender,
            overlayController: overlayController,
          ),
      child: GestureDetector(
        onTap: () {
          FocusScope.of(context).unfocus();
          overlayController.show();
        },
        child: ClipRRect(
          borderRadius: _messageBorderRadius(
            isSender,
            flightPosition,
            stackedOnTop: hasMessage,
          ),
          child: Container(
            constraints: const BoxConstraints(maxHeight: 300),
            child: AttachmentImage(
              attachment: attachment,
              imageMetadata: imageMetadata,
              isSender: isSender,
              fit: BoxFit.cover,
            ),
          ),
        ),
      ),
    );
  }
}

class _ImagePreview extends StatelessWidget {
  const _ImagePreview({
    required this.attachment,
    required this.imageMetadata,
    required this.isSender,
    required this.overlayController,
  });

  final UiAttachment attachment;
  final UiImageMetadata imageMetadata;
  final bool isSender;
  final OverlayPortalController overlayController;

  @override
  Widget build(BuildContext context) {
    return Focus(
      autofocus: true,
      onKeyEvent: (node, event) {
        if (event.logicalKey == LogicalKeyboardKey.escape &&
            event is KeyDownEvent) {
          overlayController.hide();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: GestureDetector(
        behavior: HitTestBehavior.translucent,
        child: Container(
          height: MediaQuery.of(context).size.height,
          width: MediaQuery.of(context).size.width,
          color: CustomColorScheme.of(context).backgroundBase.primary,
          child: Column(
            children: [
              AppBar(
                leading: const SizedBox.shrink(),
                actions: [
                  IconButton(
                    icon: Xmark(
                      color: CustomColorScheme.of(context).text.primary,
                      width: 32,
                    ),
                    onPressed: () {
                      overlayController.hide();
                    },
                  ),
                  const SizedBox(width: Spacings.s),
                ],
                title: Text(attachment.filename),
                centerTitle: true,
              ),
              Expanded(
                child: Center(
                  child: Padding(
                    padding: const EdgeInsets.only(
                      bottom: Spacings.l,
                      left: Spacings.s,
                      right: Spacings.s,
                    ),
                    child: AttachmentImage(
                      attachment: attachment,
                      imageMetadata: imageMetadata,
                      isSender: isSender,
                      fit: BoxFit.contain,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

BorderRadius _messageBorderRadius(
  bool isSender,
  UiFlightPosition flightPosition, {
  bool stackedOnTop = false,
}) {
  // Calculate radii
  Radius r(bool b) =>
      Radius.circular(b ? largeCornerRadius : smallCornerRadius);

  return BorderRadius.only(
    topLeft: r(isSender || flightPosition.isFirst),
    topRight: r(!isSender || flightPosition.isFirst),
    bottomLeft:
        !stackedOnTop ? r(isSender || flightPosition.isLast) : Radius.zero,
    bottomRight:
        !stackedOnTop ? r(!isSender || flightPosition.isLast) : Radius.zero,
  );
}
