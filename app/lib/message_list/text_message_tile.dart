// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async' show unawaited;

import 'package:air/core/api/markdown.dart';
import 'package:flutter/cupertino.dart'
    show
        cupertinoDesktopTextSelectionHandleControls,
        cupertinoTextSelectionHandleControls;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/attachments/attachments.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/timestamp.dart';
import 'package:air/message_list/mobile_message_actions.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/context_menu/context_menu.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/ui/typography/monospace.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
import 'package:iconoir_flutter/regular/attachment.dart';
import 'package:iconoir_flutter/regular/xmark.dart';

import 'message_renderer.dart';

const double _bubbleMaxWidthFactor = 5 / 6;
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
    final contextMenuController = useMemoized<OverlayPortalController>(
      OverlayPortalController.new,
    );
    final cursorPositionNotifier = useMemoized<ValueNotifier<Offset?>>(
      () => ValueNotifier<Offset?>(null),
    );
    final selectionAreaKey = useMemoized(
      () => GlobalKey<SelectableRegionState>(),
    );
    final bubbleKey = useMemoized(GlobalKey.new);
    final messageContainerKey = useMemoized(() => GlobalKey());
    final isDetached = useState(false);
    useEffect(() {
      return cursorPositionNotifier.dispose;
    }, [cursorPositionNotifier]);

    useEffect(() {
      return () {
        if (contextMenuController.isShowing) {
          contextMenuController.hide();
        }
      };
    }, [contextMenuController]);

    final loc = AppLocalizations.of(context);
    final plainBody = contentMessage.content.plainBody?.trim();
    final platform = Theme.of(context).platform;
    final bool isMobilePlatform =
        (platform == TargetPlatform.android || platform == TargetPlatform.iOS);

    Widget buildMessageBubble({
      required bool enableSelection,
      GlobalKey<SelectableRegionState>? selectionKey,
      GlobalKey? key,
    }) {
      Widget child = _MessageContent(
        content: contentMessage.content,
        isSender: isSender,
        flightPosition: flightPosition,
        isEdited: contentMessage.edited,
        isHidden: status == UiMessageStatus.hidden && !isRevealed.value,
        selectionAreaKey: enableSelection ? selectionKey : null,
        enableSelection: enableSelection,
      );
      if (key != null) {
        child = KeyedSubtree(key: key, child: child);
      }
      return child;
    }

    final showMessageStatus =
        isSender &&
        flightPosition.isLast &&
        status != UiMessageStatus.sending &&
        status != UiMessageStatus.hidden;

    Widget buildTimestampRow() {
      if (!flightPosition.isLast) {
        return const SizedBox.shrink();
      }

      return Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const SizedBox(height: 2),
          Row(
            mainAxisAlignment:
                isSender ? MainAxisAlignment.end : MainAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              const SizedBox(width: Spacings.s),
              Timestamp(timestamp),
              if (showMessageStatus) const SizedBox(width: Spacings.xxxs),
              if (showMessageStatus) _MessageStatus(status: status),
              const SizedBox(width: Spacings.xs),
            ],
          ),
        ],
      );
    }

    final attachments = contentMessage.content.attachments;
    final hasImageAttachment = attachments.any(
      (attachment) => attachment.imageMetadata != null,
    );

    final actions = <MessageAction>[
      if (plainBody != null && plainBody.isNotEmpty)
        MessageAction(
          label: loc.messageContextMenu_copy,
          leading: iconoir.Copy(
            width: 24,
            color: CustomColorScheme.of(context).text.primary,
          ),
          onSelected: () {
            Clipboard.setData(ClipboardData(text: plainBody));
          },
        ),
      if (isSender && !hasImageAttachment)
        MessageAction(
          label: loc.messageContextMenu_edit,
          leading: iconoir.EditPencil(
            width: 24,
            color: CustomColorScheme.of(context).text.primary,
          ),
          onSelected: () {
            context.read<ChatDetailsCubit>().editMessage(messageId: messageId);
          },
        ),
    ];

    final menuItems =
        actions
            .map(
              (action) => ContextMenuItem(
                label: action.label,
                leading: action.leading,
                onPressed: action.onSelected,
              ),
            )
            .toList();

    Widget buildMessageShell({
      required VoidCallback? onLongPress,
      GestureTapDownCallback? onSecondaryTapDown,
      required bool enableSelection,
      required GlobalKey messageKey,
      required bool detached,
      GlobalKey? bubbleRenderKey,
    }) {
      final bubble = buildMessageBubble(
        enableSelection: enableSelection,
        selectionKey: enableSelection ? selectionAreaKey : null,
        key: bubbleRenderKey,
      );
      final timestampRow = buildTimestampRow();

      return Container(
        key: messageKey,
        padding: EdgeInsets.only(
          top: flightPosition.isFirst ? 5 : 0,
          bottom: flightPosition.isLast ? 5 : 0,
        ),
        child: Column(
          crossAxisAlignment:
              isSender ? CrossAxisAlignment.end : CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            MouseRegion(
              cursor: SystemMouseCursors.basic,
              child: AnimatedOpacity(
                opacity: detached ? 0.0 : 1.0,
                duration: const Duration(milliseconds: 120),
                child: IgnorePointer(
                  ignoring: detached,
                  child: GestureDetector(
                    behavior: HitTestBehavior.deferToChild,
                    onTap: () => isRevealed.value = true,
                    onLongPress: onLongPress,
                    onSecondaryTapDown: onSecondaryTapDown,
                    child: bubble,
                  ),
                ),
              ),
            ),
            timestampRow,
          ],
        ),
      );
    }

    Widget wrapWithBubbleWidth(Widget child) {
      return LayoutBuilder(
        builder: (context, constraints) {
          final hasFiniteWidth = constraints.maxWidth.isFinite;
          final double maxWidth =
              hasFiniteWidth
                  ? constraints.maxWidth * _bubbleMaxWidthFactor
                  : double.infinity;
          final alignment =
              isSender ? Alignment.centerRight : Alignment.centerLeft;
          final boxConstraints =
              hasFiniteWidth
                  ? BoxConstraints(maxWidth: maxWidth)
                  : const BoxConstraints();
          return Align(
            alignment: alignment,
            child: ConstrainedBox(constraints: boxConstraints, child: child),
          );
        },
      );
    }

    if (isMobilePlatform) {
      return wrapWithBubbleWidth(
        buildMessageShell(
          onLongPress:
              actions.isEmpty
                  ? null
                  : () {
                    final bubbleContext = bubbleKey.currentContext;
                    if (bubbleContext == null) return;
                    final renderObject = bubbleContext.findRenderObject();
                    if (renderObject is! RenderBox || !renderObject.hasSize) {
                      return;
                    }
                    final origin = renderObject.localToGlobal(Offset.zero);
                    final anchorRect = origin & renderObject.size;
                    final overlayBubble = buildMessageBubble(
                      enableSelection: false,
                    );
                    ContextMenu.closeActiveMenu();
                    isDetached.value = true;
                    final future = showMobileMessageActions(
                      context: context,
                      anchorRect: anchorRect,
                      actions: actions,
                      messageContent: overlayBubble,
                      alignEnd: isSender,
                    );
                    unawaited(
                      future.whenComplete(() {
                        isDetached.value = false;
                      }),
                    );
                  },
          onSecondaryTapDown: null,
          enableSelection: false,
          messageKey: messageContainerKey,
          detached: isDetached.value,
          bubbleRenderKey: bubbleKey,
        ),
      );
    }

    return wrapWithBubbleWidth(
      ContextMenu(
        direction:
            isSender ? ContextMenuDirection.left : ContextMenuDirection.right,
        width: 200,
        offset: const Offset(Spacings.xxs, 0),
        controller: contextMenuController,
        menuItems: menuItems,
        cursorPosition: cursorPositionNotifier,
        child: buildMessageShell(
          onLongPress: null,
          onSecondaryTapDown:
              actions.isEmpty
                  ? null
                  : (details) {
                    selectionAreaKey.currentState?.clearSelection();
                    if (contextMenuController.isShowing) {
                      contextMenuController.hide();
                    }
                    ContextMenu.closeActiveMenu();
                    cursorPositionNotifier.value = details.globalPosition;
                    contextMenuController.show();
                  },
          enableSelection: true,
          messageKey: messageContainerKey,
          detached: false,
          bubbleRenderKey: bubbleKey,
        ),
      ),
    );
  }
}

class _MessageStatus extends StatelessWidget {
  const _MessageStatus({required this.status});

  final UiMessageStatus status;

  @override
  Widget build(BuildContext context) {
    final readReceiptsEnabled = context.select(
      (UserSettingsCubit cubit) => cubit.state.readReceipts,
    );
    return DoubleCheckIcon(
      size: LabelFontSize.small2.size,
      singleCheckIcon: status == UiMessageStatus.sent,
      inverted: readReceiptsEnabled && status == UiMessageStatus.read,
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
    required this.selectionAreaKey,
    required this.enableSelection,
  });

  final UiMimiContent content;
  final bool isSender;
  final UiFlightPosition flightPosition;
  final bool isEdited;
  final bool isHidden;
  final GlobalKey<SelectableRegionState>? selectionAreaKey;
  final bool enableSelection;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final platform = Theme.of(context).platform;
    final TextSelectionControls? selectionControls =
        enableSelection
            ? switch (platform) {
              TargetPlatform.android ||
              TargetPlatform.fuchsia => materialTextSelectionHandleControls,
              TargetPlatform.linux ||
              TargetPlatform.windows => desktopTextSelectionHandleControls,
              TargetPlatform.iOS => cupertinoTextSelectionHandleControls,
              TargetPlatform.macOS =>
                cupertinoDesktopTextSelectionHandleControls,
            }
            : null;

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

    final Widget contentColumn = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: contentElements,
    );

    final Widget selectableChild =
        enableSelection
            ? SelectableRegion(
              key: selectionAreaKey,
              selectionControls: selectionControls!,
              magnifierConfiguration:
                  TextMagnifier.adaptiveMagnifierConfiguration,
              contextMenuBuilder: (context, _) => const SizedBox.shrink(),
              child: contentColumn,
            )
            : SelectionContainer.disabled(child: contentColumn);

    return Padding(
      padding: const EdgeInsets.only(bottom: 1.5),
      child: Container(
        alignment:
            isSender
                ? AlignmentDirectional.topEnd
                : AlignmentDirectional.topStart,
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
                    selectableChild,
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
