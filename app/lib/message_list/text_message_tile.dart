// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async' show unawaited;
import 'dart:io';

import 'package:air/attachments/attachments.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/message_list/mobile_message_actions.dart';
import 'package:air/message_list/timestamp.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/context_menu/context_menu.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/icons/app_icon.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/util/platform.dart';
import 'package:air/widgets/widgets.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:share_plus/share_plus.dart';

import 'image_viewer.dart';
import 'message_renderer.dart';

final _log = Logger('MessageTile');

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
  final DateTime timestamp;
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
  final DateTime timestamp;
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
        platform == TargetPlatform.android || platform == TargetPlatform.iOS;
    final bool isDesktopPlatform =
        platform == TargetPlatform.macOS ||
        platform == TargetPlatform.linux ||
        platform == TargetPlatform.windows;

    Widget buildMessageBubble({required bool enableSelection, GlobalKey? key}) {
      Widget child = _MessageContent(
        content: contentMessage.content,
        isSender: isSender,
        flightPosition: flightPosition,
        isEdited: contentMessage.edited,
        isHidden: status == UiMessageStatus.hidden && !isRevealed.value,
        enableSelection: enableSelection,
      );
      if (key != null) {
        child = KeyedSubtree(key: key, child: child);
      }
      return child;
    }

    final showMessageStatus =
        isSender && flightPosition.isLast && status != UiMessageStatus.hidden;

    final isSendingOrError =
        status == UiMessageStatus.error || status == UiMessageStatus.sending;

    Widget buildTimestampRow() {
      if (!flightPosition.isLast) {
        return const SizedBox.shrink();
      }

      return SelectionContainer.disabled(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const SizedBox(height: 2),
            Row(
              mainAxisAlignment: isSender
                  ? MainAxisAlignment.end
                  : MainAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                const SizedBox(width: Spacings.s),
                if (!isSendingOrError) Timestamp(timestamp),
                if (showMessageStatus) const SizedBox(width: Spacings.xxxs),
                if (showMessageStatus && status == UiMessageStatus.error)
                  Text(
                    style: TextStyle(
                      color: CustomColorScheme.of(context).function.warning,
                    ),
                    loc.messageBubble_failedToSend,
                  ),
                if (showMessageStatus && status == UiMessageStatus.sending)
                  Text(
                    style: TextStyle(
                      color: CustomColorScheme.of(context).text.tertiary,
                    ),
                    loc.messageBubble_sending,
                  ),
                if (showMessageStatus && isSendingOrError)
                  const SizedBox(width: Spacings.xxxs),
                if (showMessageStatus) _MessageStatus(status: status),
                const SizedBox(width: Spacings.xs),
              ],
            ),
          ],
        ),
      );
    }

    final attachments = contentMessage.content.attachments;

    final colors = CustomColorScheme.of(context);

    final actions = <MessageAction>[
      if (plainBody != null && plainBody.isNotEmpty)
        MessageAction(
          label: loc.messageContextMenu_copy,
          leading: AppIcon(
            type: AppIconType.copy,
            size: 24,
            color: colors.text.primary,
          ),
          onSelected: () {
            Clipboard.setData(ClipboardData(text: plainBody));
          },
        ),
      if (isSender && attachments.isEmpty)
        MessageAction(
          label: loc.messageContextMenu_edit,
          leading: AppIcon(
            type: AppIconType.editPencil,
            size: 24,
            color: colors.text.primary,
          ),
          onSelected: () {
            context.read<ChatDetailsCubit>().editMessage(messageId: messageId);
          },
        ),
      if (attachments.isNotEmpty && !Platform.isIOS)
        MessageAction(
          label: loc.messageContextMenu_save,
          leading: AppIcon(
            type: AppIconType.download,
            size: 24,
            color: colors.text.primary,
          ),
          onSelected: () => _handleFileSave(context, attachments.first),
        ),
      if (attachments.isNotEmpty && Platform.isIOS)
        MessageAction(
          label: loc.messageContextMenu_share,
          leading: AppIcon(
            type: AppIconType.shareIos,
            size: 24,
            color: colors.text.primary,
          ),
          onSelected: () => _handleFileShare(context, attachments),
        ),
    ];

    final menuItems = actions
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
          crossAxisAlignment: isSender
              ? CrossAxisAlignment.end
              : CrossAxisAlignment.start,
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
          final double maxWidth = hasFiniteWidth
              ? constraints.maxWidth * _bubbleMaxWidthFactor
              : double.infinity;
          final alignment = isSender
              ? Alignment.centerRight
              : Alignment.centerLeft;
          final boxConstraints = hasFiniteWidth
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
          onLongPress: actions.isEmpty
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
        direction: isSender
            ? ContextMenuDirection.left
            : ContextMenuDirection.right,
        width: 200,
        offset: const Offset(Spacings.xxs, 0),
        controller: contextMenuController,
        menuItems: menuItems,
        cursorPosition: cursorPositionNotifier,
        child: buildMessageShell(
          onLongPress: null,
          onSecondaryTapDown: actions.isEmpty
              ? null
              : (details) {
                  if (contextMenuController.isShowing) {
                    contextMenuController.hide();
                  }
                  ContextMenu.closeActiveMenu();
                  cursorPositionNotifier.value = details.globalPosition;
                  contextMenuController.show();
                },
          enableSelection: isDesktopPlatform,
          messageKey: messageContainerKey,
          detached: false,
          bubbleRenderKey: bubbleKey,
        ),
      ),
    );
  }

  void _handleFileSave(BuildContext context, UiAttachment attachment) async {
    if (Platform.isAndroid) {
      // Android uses platform-specific code to write data directly into a provided URI
      final attachmentsRepository = context.read<AttachmentsRepository>();
      final data = await attachmentsRepository.loadAttachment(
        attachmentId: attachment.attachmentId,
      );
      if (data == null) {
        _log.severe("Missing attachment data");
        return;
      }
      await saveFileAndroid(
        fileName: attachment.filename,
        mimeType: attachment.contentType,
        data: data,
      );
    } else if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
      // On Desktop, we save the attachment in Rust after getting a path from the platform-specific
      // dialog
      final attachmentsRepository = context.read<AttachmentsRepository>();
      final location = await getSaveLocation(
        suggestedName: attachment.filename,
      );
      if (location == null) return;
      final path = location.path;

      try {
        await attachmentsRepository.saveAttachment(
          attachmentId: attachment.attachmentId,
          path: path,
        );
      } catch (e, stackTrace) {
        _log.severe("Failed to save attachment: $e", e, stackTrace);
        if (context.mounted) {
          final loc = AppLocalizations.of(context);
          showErrorBanner(context, loc.messageContextMenu_saveError);
        }
        return;
      }
    } else if (Platform.isIOS) {
      throw UnsupportedError("iOS does not support storing files");
    } else {
      throw UnsupportedError("Unsupported platform");
    }

    // TODO: Snackbar overlaps with the composer, so we need a better solution
    if (context.mounted) {
      final loc = AppLocalizations.of(context);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          duration: const Duration(seconds: 1),
          content: Text(loc.messageContextMenu_saveConfirmation),
        ),
      );
    }
  }

  void _handleFileShare(
    BuildContext context,
    List<UiAttachment> attachments,
  ) async {
    final attachmentsRepository = context.read<AttachmentsRepository>();

    final futures = attachments.map((attachment) async {
      final data = await attachmentsRepository.loadAttachment(
        attachmentId: attachment.attachmentId,
      );
      if (data == null) return null;
      return XFile.fromData(data);
    });

    final files = (await Future.wait(futures)).whereType<XFile>().toList();

    final params = ShareParams(
      files: files,
      fileNameOverrides: attachments.map((e) => e.filename).toList(),
    );
    SharePlus.instance.share(params);
  }
}

class RotatingSendIcon extends StatefulWidget {
  const RotatingSendIcon({super.key});

  @override
  State<RotatingSendIcon> createState() => _RotatingSendIconState();
}

class _RotatingSendIconState extends State<RotatingSendIcon>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      duration: const Duration(seconds: 1),
      vsync: this,
    )..repeat();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RotationTransition(
      turns: _controller,
      child: AppIcon(
        type: AppIconType.refreshDouble,
        size: LabelFontSize.small2.size,
        color: CustomColorScheme.of(context).text.tertiary,
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
    if (status == UiMessageStatus.sending) {
      return const RotatingSendIcon();
    }
    if (status == UiMessageStatus.error) {
      return AppIcon(
        type: AppIconType.warningCircle,
        size: LabelFontSize.small2.size,
        color: CustomColorScheme.of(context).function.warning,
      );
    }
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
    required this.enableSelection,
  });

  final UiMimiContent content;
  final bool isSender;
  final UiFlightPosition flightPosition;
  final bool isEdited;
  final bool isHidden;
  final bool enableSelection;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final bool isDeleted = content.replaces != null && content.content == null;
    final List<Widget> columnChildren = [];

    if (isHidden) {
      columnChildren.add(
        SelectionContainer.disabled(
          child: Padding(
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
        ),
      );
    } else {
      final List<Widget> selectableBlocks = [];

      if (isDeleted) {
        selectableBlocks.add(
          Padding(
            padding: _messagePadding,
            child: buildBlockElement(
              context,
              BlockElement.error(loc.textMessage_deleted),
              isSender,
            ),
          ),
        );
      }

      if (content.attachments.firstOrNull case final attachment?) {
        final Widget attachmentWidget = switch (attachment.imageMetadata) {
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
        };
        columnChildren.add(
          SelectionContainer.disabled(child: attachmentWidget),
        );
      }

      selectableBlocks.addAll(
        (content.content?.elements ?? []).map(
          (inner) => Padding(
            padding: _messagePadding.copyWith(bottom: isEdited ? 0 : null),
            child: buildBlockElement(context, inner.element, isSender),
          ),
        ),
      );

      if (selectableBlocks.isNotEmpty) {
        final textColumn = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: selectableBlocks,
        );
        final Widget selectableChild = enableSelection
            ? SelectableRegion(
                selectionControls: emptyTextSelectionControls,
                contextMenuBuilder: (context, _) => const SizedBox.shrink(),
                child: textColumn,
              )
            : SelectionContainer.disabled(child: textColumn);
        columnChildren.add(selectableChild);
      }
    }

    return Padding(
      padding: const EdgeInsets.only(bottom: 1.5),
      child: Container(
        alignment: isSender
            ? AlignmentDirectional.topEnd
            : AlignmentDirectional.topStart,
        child: DecoratedBox(
          decoration: BoxDecoration(
            borderRadius: _messageBorderRadius(isSender, flightPosition),
            color: isSender
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
                      children: columnChildren,
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
                            style: Theme.of(context).textTheme.bodySmall!
                                .copyWith(
                                  color: isSender
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
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: () {
            unawaited(
              context.read<NavigationCubit>().openMemberDetails(sender),
            );
          },
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              UserAvatar(userId: sender, size: Spacings.m),
              const SizedBox(width: Spacings.xs),
              _DisplayName(
                displayName: profile.displayName,
                isSender: isSender,
              ),
            ],
          ),
        ),
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
    return SelectionContainer.disabled(
      child: Text(
        text,
        style: TextTheme.of(context).labelSmall!.copyWith(
          color: CustomColorScheme.of(context).text.tertiary,
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
    return Padding(
      padding: _messagePadding,
      child: AttachmentFile(
        attachment: attachment,
        isSender: isSender,
        color: isSender
            ? CustomColorScheme.of(context).message.selfText
            : CustomColorScheme.of(context).message.otherText,
      ),
    );
  }
}

class _ImageAttachmentContent extends StatelessWidget {
  const _ImageAttachmentContent({
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

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: () {
        FocusScope.of(context).unfocus();
        HapticFeedback.mediumImpact();
        Navigator.of(context).push(imageViewerRoute(attachment: attachment));
      },
      child: Hero(
        tag: imageViewerHeroTag(attachment),
        transitionOnUserGestures: true,
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
    bottomLeft: !stackedOnTop
        ? r(isSender || flightPosition.isLast)
        : Radius.zero,
    bottomRight: !stackedOnTop
        ? r(!isSender || flightPosition.isLast)
        : Radius.zero,
  );
}
