// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async' show Timer, unawaited;
import 'dart:io';
import 'dart:math' as math;

import 'package:air/attachments/attachments.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/jumbo_emoji.dart';
import 'package:air/message_list/emoji_picker.dart';
import 'package:air/message_list/emoji_repository.dart';
import 'package:air/message_list/message_composer.dart';
import 'package:air/message_list/message_list_cubit.dart';
import 'package:air/message_list/message_reactions.dart';
import 'package:air/message_list/mobile_message_actions.dart';
import 'package:air/message_list/timestamp.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/ds/components/context_menu/context_menu.dart';
import 'package:air/ds/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/util/platform.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/widgets/widgets.dart';
import 'package:file_selector/file_selector.dart';
import 'package:path/path.dart' as p;
import 'package:flutter/gestures.dart'
    show EagerGestureRecognizer, kSecondaryMouseButton;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:share_plus/share_plus.dart';

import 'image_viewer.dart';
import 'jump_highlight.dart';
import 'message_renderer.dart';
import 'swipe_to_reply.dart';

final _log = Logger('MessageTile');

const double _bubbleMaxWidthFactor = 5 / 6;
// Match the hover react button to a single-line message bubble.
final double _hoverReactSize =
    BodyFontSize.base.size * BodyFontSize.lineHeight +
    messageVerticalPadding * 2;
// Width the hover affordance occupies beside the bubble: the reply and react
// buttons, the gap between them, and the gap to the bubble.
final double _hoverAffordanceWidth = 2 * _hoverReactSize + 2 * Spacing.px8;
const double largeCornerRadius = Spacing.px20;
const double smallCornerRadius = Spacing.px8;
const double messageHorizontalPadding = Spacing.px16;
const double messageVerticalPadding = Spacing.px8;
const double senderAvatarSize = Spacing.px32;
const double senderAvatarVerticalOffset = Spacing.px4;
const double senderLabelBottomGap = Spacing.px4 / 2;
const double incomingContentInset =
    senderAvatarSize + Spacing.px12 + messageHorizontalPadding;

const _messagePadding = EdgeInsets.symmetric(
  horizontal: messageHorizontalPadding,
  vertical: messageVerticalPadding,
);

/// Places [affordance] beside [child], vertically centered on it (the trailing
/// hover button sits centered on the bubble, not the bubble + metadata).
Widget _withTrailingAffordance({
  required bool isSender,
  required Widget child,
  Widget? affordance,
}) {
  if (affordance == null) return child;
  // The child must be Flexible so the Row hands it a bounded width.
  final flexibleChild = Flexible(child: child);
  return Row(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.center,
    children: isSender
        ? [affordance, flexibleChild]
        : [flexibleChild, affordance],
  );
}

class WrapWithBubbleWidth extends StatelessWidget {
  const WrapWithBubbleWidth({
    super.key,
    required this.isSender,
    this.affordanceWidth = 0,
    required this.child,
  });

  final bool isSender;

  /// Width reserved by the hover affordance beside the bubble. Granted on top
  /// of the bubble width factor so the affordance lives in the free margin,
  /// the bubble only shrinks when the row is too narrow to fit both.
  final double affordanceWidth;

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final hasFiniteWidth = constraints.maxWidth.isFinite;
        final double maxWidth = hasFiniteWidth
            ? math.min(
                constraints.maxWidth,
                constraints.maxWidth * _bubbleMaxWidthFactor + affordanceWidth,
              )
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
}

class TextMessageTile extends StatelessWidget {
  const TextMessageTile({
    required this.messageId,
    required this.contentMessage,
    required this.inReplyToMessage,
    required this.timestamp,
    required this.flightPosition,
    required this.status,
    required this.isSender,
    required this.showSender,
    required this.reactions,
    required this.ownUserId,
    super.key,
  });

  final MessageId messageId;
  final UiContentMessage contentMessage;
  final UiInReplyToMessage? inReplyToMessage;
  final DateTime timestamp;
  final UiFlightPosition flightPosition;
  final UiMessageStatus status;
  final bool isSender;
  final bool showSender;
  final List<UiReaction> reactions;
  final UiUserId ownUserId;

  @override
  Widget build(BuildContext context) {
    final showParticipantDetails = showSender && !isSender;
    if (showParticipantDetails) {
      return _IncomingMessageTile(
        messageId: messageId,
        contentMessage: contentMessage,
        inReplyToMessage: inReplyToMessage,
        timestamp: timestamp,
        flightPosition: flightPosition,
        status: status,
        reactions: reactions,
        ownUserId: ownUserId,
      );
    }

    return _MessageView(
      messageId: messageId,
      contentMessage: contentMessage,
      inReplyToMessage: inReplyToMessage,
      timestamp: timestamp,
      isSender: isSender,
      flightPosition: flightPosition,
      status: status,
      showMetadata: true,
      showSenderLabel: showSender,
      reactions: reactions,
      ownUserId: ownUserId,
    );
  }
}

class _IncomingMessageTile extends StatelessWidget {
  const _IncomingMessageTile({
    required this.messageId,
    required this.contentMessage,
    required this.inReplyToMessage,
    required this.timestamp,
    required this.flightPosition,
    required this.status,
    required this.reactions,
    required this.ownUserId,
  });

  final MessageId messageId;
  final UiContentMessage contentMessage;
  final UiInReplyToMessage? inReplyToMessage;
  final DateTime timestamp;
  final UiFlightPosition flightPosition;
  final UiMessageStatus status;
  final List<UiReaction> reactions;
  final UiUserId ownUserId;

  @override
  Widget build(BuildContext context) {
    final showSenderLabel = flightPosition.isFirst;
    final showAvatar = flightPosition.isLast;
    final senderProfile = showSenderLabel
        ? context.select(
            (UsersCubit cubit) =>
                cubit.state.profile(userId: contentMessage.sender),
          )
        : null;
    void openMemberDetails() {
      unawaited(
        context.read<NavigationCubit>().openMemberDetails(
          contentMessage.sender,
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (showSenderLabel)
          Padding(
            padding: const EdgeInsets.only(
              top: Spacing.px12,
              bottom: senderLabelBottomGap,
              left: incomingContentInset,
            ),
            child: _SenderHeader(
              displayName: senderProfile!.displayName,
              onTap: openMemberDetails,
            ),
          ),
        Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Padding(
              padding: EdgeInsets.only(
                bottom: reactionsReservedBelow(context, reactions.isNotEmpty),
              ),
              child: SizedBox(
                width: senderAvatarSize,
                child: showAvatar
                    ? Transform.translate(
                        offset: const Offset(0, -senderAvatarVerticalOffset),
                        child: _SenderAvatar(
                          sender: contentMessage.sender,
                          onTap: openMemberDetails,
                          size: senderAvatarSize,
                        ),
                      )
                    : const SizedBox.shrink(),
              ),
            ),
            const SizedBox(width: Spacing.px12),
            Expanded(
              child: _MessageView(
                messageId: messageId,
                contentMessage: contentMessage,
                inReplyToMessage: inReplyToMessage,
                timestamp: timestamp,
                isSender: false,
                flightPosition: flightPosition,
                status: status,
                showMetadata: false,
                showSenderLabel: showSenderLabel,
                reactions: reactions,
                ownUserId: ownUserId,
              ),
            ),
          ],
        ),
        if (flightPosition.isLast)
          Padding(
            padding: const EdgeInsets.only(left: incomingContentInset),
            child: _MessageMetadataRow(
              timestamp: timestamp,
              isSender: false,
              flightPosition: flightPosition,
              status: status,
            ),
          ),
        if (flightPosition.isLast) const SizedBox(height: Spacing.px8),
      ],
    );
  }
}

class _MessageView extends HookWidget {
  const _MessageView({
    required this.messageId,
    required this.contentMessage,
    required this.inReplyToMessage,
    required this.timestamp,
    required this.flightPosition,
    required this.isSender,
    required this.status,
    required this.showMetadata,
    required this.showSenderLabel,
    required this.reactions,
    required this.ownUserId,
  });

  final MessageId messageId;
  final UiContentMessage contentMessage;
  final UiInReplyToMessage? inReplyToMessage;
  final DateTime timestamp;
  final UiFlightPosition flightPosition;
  final bool isSender;
  final UiMessageStatus status;
  final bool showMetadata;
  final bool showSenderLabel;
  final List<UiReaction> reactions;
  final UiUserId ownUserId;

  @override
  Widget build(BuildContext context) {
    final isRevealed = useState(false);
    final contextMenuController = useMemoized<OverlayPortalController>(
      OverlayPortalController.new,
    );
    final cursorPositionNotifier = useMemoized<ValueNotifier<Offset?>>(
      () => ValueNotifier<Offset?>(null),
    );
    final messageBubbleKey = useMemoized(() => GlobalKey());
    final reactButtonKey = useMemoized(() => GlobalKey());
    final isDetached = useState(false);
    // A ValueNotifier (not useState) so hover changes rebuild only the hover
    // affordance via a ValueListenableBuilder, not the whole message tile (and
    // its reaction chips, which re-measure on every build).
    final isHovered = useMemoized(() => ValueNotifier(false));
    useEffect(() {
      return () {
        cursorPositionNotifier.dispose();
        isHovered.dispose();
      };
    }, [cursorPositionNotifier, isHovered]);

    useEffect(() {
      return () {
        if (contextMenuController.isShowing) {
          contextMenuController.hide();
        }
      };
    }, [contextMenuController]);

    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final plainBody = contentMessage.content.plainBody?.trim();
    final platform = Theme.of(context).platform;
    final bool isMobilePlatform =
        platform == TargetPlatform.android || platform == TargetPlatform.iOS;
    final bool isDesktopPlatform =
        platform == TargetPlatform.macOS ||
        platform == TargetPlatform.linux ||
        platform == TargetPlatform.windows;

    // Persisted default skin tone applied to reactions and the picker.
    final skinToneIndex = context.select(
      (UserSettingsCubit cubit) => cubit.state.defaultEmojiSkinTone,
    );
    final skinTone = EmojiSkinVariation
        .values[skinToneIndex.clamp(0, EmojiSkinVariation.values.length - 1)];

    Rect? globalRectOf(GlobalKey key) {
      final renderObject = key.currentContext?.findRenderObject();
      if (renderObject is! RenderBox || !renderObject.hasSize) {
        return null;
      }
      final origin = renderObject.localToGlobal(Offset.zero);
      return origin & renderObject.size;
    }

    Rect? reactionAnchorRect() => globalRectOf(messageBubbleKey);

    void sendReaction(String emoji) {
      context.read<ChatDetailsCubit>().sendReaction(
        messageId: messageId,
        emoji: emoji,
      );
    }

    // Pass [barrierColor] transparent when the picker opens on top of the
    // quick-reaction strip, whose barrier stays alive underneath so the dim
    // doesn't flicker.
    Future<void> openFullEmojiPicker({Color? barrierColor}) async {
      // Capture cubits before the await so the picker can persist tone changes.
      final settings = context.read<UserSettingsCubit>();
      final userCubit = context.read<UserCubit>();
      void onSkinToneChanged(EmojiSkinVariation tone) {
        unawaited(
          settings.setDefaultEmojiSkinTone(
            userCubit: userCubit,
            value: tone.index,
          ),
        );
      }

      final emoji = isMobilePlatform
          ? await showEmojiPickerSheet(
              context: context,
              initialSkinTone: skinTone,
              onSkinToneChanged: onSkinToneChanged,
              barrierColor: barrierColor,
            )
          : await showEmojiPickerPopover(
              context: context,
              initialSkinTone: skinTone,
              onSkinToneChanged: onSkinToneChanged,
              barrierColor: barrierColor,
            );
      if (emoji != null && context.mounted) {
        sendReaction(emoji);
      }
    }

    void openReactionMenu({GlobalKey? anchorKey}) {
      // Anchor the bar to the trigger so it opens centered above it: the hover
      // react button, else the right-click point, else the message bubble as a
      // last resort (e.g. mobile double-tap).
      final cursor = cursorPositionNotifier.value;
      final anchorRect =
          (anchorKey != null ? globalRectOf(anchorKey) : null) ??
          (cursor != null ? cursor & Size.zero : null) ??
          reactionAnchorRect();
      if (anchorRect == null) return;
      unawaited(
        showQuickReactionMenu(
          context: context,
          anchorRect: anchorRect,
          skinTone: skinTone,
          onReact: sendReaction,
          onMore: () => openFullEmojiPicker(barrierColor: Colors.transparent),
        ),
      );
    }

    void showReactors(String? tappedEmoji) {
      final chatDetailsCubit = context.read<ChatDetailsCubit>();
      unawaited(
        showWhoReactedSheet(
          context: context,
          reactions: reactions,
          ownUserId: ownUserId,
          initialEmoji: tappedEmoji,
          onRemove: (emoji) => chatDetailsCubit.deleteReaction(
            messageId: messageId,
            emoji: emoji,
          ),
        ),
      );
    }

    Widget buildMessageBubble({required bool enableSelection}) {
      return _MessageContent(
        messageId: messageId,
        content: contentMessage.content,
        inReplyToMessage: inReplyToMessage,
        isSender: isSender,
        senderId: contentMessage.sender,
        flightPosition: flightPosition,
        isEdited: contentMessage.edited,
        isHidden: status == UiMessageStatus.hidden && !isRevealed.value,
        isError: status == UiMessageStatus.error,
        enableSelection: enableSelection,
      );
    }

    final attachments = contentMessage.content.attachments;
    final isDeleted = contentMessage.content.isDeleted;
    final isReplyable = !isDeleted && status != UiMessageStatus.error;

    const iconSize = 16.0;

    final actions = [
      if (isReplyable)
        MessageAction(
          label: loc.messageContextMenu_reply,
          leading: const AppIcon.cornerLeft(size: iconSize),
          onSelected: () {
            context.read<ChatDetailsCubit>().replyToMessage(
              messageId: messageId,
            );
          },
        ),
      if (plainBody != null && plainBody.isNotEmpty)
        MessageAction(
          label: loc.messageContextMenu_copy,
          leading: const AppIcon.copy(size: iconSize),
          onSelected: () {
            Clipboard.setData(ClipboardData(text: plainBody));
          },
        ),
      if (isSender && attachments.isEmpty && !isDeleted)
        MessageAction(
          label: loc.messageContextMenu_edit,
          leading: const AppIcon.pencil(size: iconSize),
          onSelected: () {
            context.read<ChatDetailsCubit>().editMessage(messageId: messageId);
          },
        ),
      if (!isDeleted)
        MessageAction(
          label: loc.messageContextMenu_delete,
          leading: AppIcon.trash(size: iconSize, color: colors.function.danger),
          isDestructive: true,
          insertSeparatorBefore: true,
          onSelected: () => isSender
              ? _showDeleteMessageDialog(context: context, messageId: messageId)
              : _showDeleteForMeDialog(context: context, messageId: messageId),
        ),
      if (isDeleted)
        MessageAction(
          label: loc.messageContextMenu_delete,
          leading: AppIcon.trash(size: iconSize, color: colors.function.danger),
          isDestructive: true,
          onSelected: () =>
              _showDeleteForMeDialog(context: context, messageId: messageId),
        ),
      if (attachments.isNotEmpty && !Platform.isIOS)
        MessageAction(
          label: loc.messageContextMenu_save,
          leading: const AppIcon.download(size: iconSize),
          onSelected: () => _handleFileSave(context, attachments.first),
        ),
      if (attachments.isNotEmpty && Platform.isIOS)
        MessageAction(
          label: loc.messageContextMenu_share,
          leading: const AppIcon.share(size: iconSize),
          onSelected: () => _handleFileShare(context, attachments),
        ),
    ];

    final menuItems = <ContextMenuEntry>[];
    if (isReplyable) {
      menuItems.add(
        ContextMenuItem(
          label: loc.messageList_reactions_react,
          leading: const AppIcon.smilePlus(size: iconSize),
          onPressed: openReactionMenu,
        ),
      );
    }
    for (final action in actions) {
      if (action.insertSeparatorBefore) {
        menuItems.add(const ContextMenuSeparator());
      }
      menuItems.add(
        ContextMenuItem(
          label: action.label,
          leading: action.leading,
          onPressed: action.onSelected,
          isDestructive: action.isDestructive,
        ),
      );
    }

    final metadata = Padding(
      padding: EdgeInsets.only(left: isSender ? 0 : messageHorizontalPadding),
      child: _MessageMetadataRow(
        timestamp: timestamp,
        isSender: isSender,
        flightPosition: flightPosition,
        status: status,
      ),
    );

    Widget buildMessageShell({
      required VoidCallback? onLongPress,
      GestureTapDownCallback? onSecondaryTapDown,
      required bool enableSelection,
      required GlobalKey messageKey,
      required bool detached,
      required bool includeMetadata,
      Widget? trailingAffordance,
    }) {
      Widget bubble = KeyedSubtree(
        key: messageKey,
        child: buildMessageBubble(enableSelection: enableSelection),
      );
      if (!enableSelection && isReplyable) {
        bubble = SwipeToReplyBubble(
          icon: AppIcon.cornerLeft(size: 16, color: colors.text.secondary),
          child: bubble,
        );
      }

      bubble = BubbleWithReactions(
        reactions: reactions,
        isSender: isSender,
        ownUserId: ownUserId,
        onTap: showReactors,
        bubble: bubble,
      );

      return Container(
        padding: EdgeInsets.only(
          top: flightPosition.isFirst ? Spacing.px4 : 0,
          bottom: includeMetadata && flightPosition.isLast ? 5 : 0,
        ),
        child: Column(
          crossAxisAlignment: isSender
              ? CrossAxisAlignment.end
              : CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            _withTrailingAffordance(
              isSender: isSender,
              affordance: trailingAffordance,
              child: MouseRegion(
                cursor: SystemMouseCursors.basic,
                child: AnimatedOpacity(
                  opacity: detached ? 0.0 : 1.0,
                  duration: const Duration(milliseconds: 120),
                  child: IgnorePointer(
                    ignoring: detached,
                    // Right-click: handled via raw pointer events to
                    // bypass the gesture arena (won by
                    // _EagerSecondaryClickRecognizer).
                    child: Listener(
                      onPointerDown: onSecondaryTapDown != null
                          ? (event) {
                              if (event.buttons == kSecondaryMouseButton) {
                                onSecondaryTapDown(
                                  TapDownDetails(
                                    globalPosition: event.position,
                                    localPosition: event.localPosition,
                                  ),
                                );
                              }
                            }
                          : null,
                      // Tap and long-press: handled via the
                      // gesture arena as usual.
                      child: GestureDetector(
                        behavior: HitTestBehavior.deferToChild,
                        onTap:
                            (status == UiMessageStatus.hidden &&
                                !isRevealed.value)
                            ? () => isRevealed.value = true
                            : null,
                        // Mobile: double-tap a message to react. On desktop,
                        // double-click is reserved for text selection.
                        onDoubleTap: (isMobilePlatform && isReplyable)
                            ? openReactionMenu
                            : null,
                        onLongPress: onLongPress,
                        child: bubble,
                      ),
                    ),
                  ),
                ),
              ),
            ),
            if (includeMetadata) metadata,
          ],
        ),
      );
    }

    if (isMobilePlatform) {
      final wrappedBubble = WrapWithBubbleWidth(
        isSender: isSender,
        child: buildMessageShell(
          onLongPress: actions.isEmpty
              ? null
              : () {
                  final shellContext = messageBubbleKey.currentContext;
                  if (shellContext == null) return;
                  final renderObject = shellContext.findRenderObject();
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
                    reactionSkinTone: skinTone,
                    onReact: isReplyable ? sendReaction : null,
                    onReactMore: isReplyable
                        ? () => unawaited(openFullEmojiPicker())
                        : null,
                  );
                  unawaited(
                    future.whenComplete(() {
                      isDetached.value = false;
                    }),
                  );
                },
          onSecondaryTapDown: null,
          enableSelection: false,
          messageKey: messageBubbleKey,
          detached: isDetached.value,
          includeMetadata: showMetadata,
        ),
      );

      return isReplyable
          ? SwipeToReplyScope(
              onReply: () {
                context.read<ChatDetailsCubit>().replyToMessage(
                  messageId: messageId,
                );
              },
              child: wrappedBubble,
            )
          : wrappedBubble;
    }

    // Reveal-on-hover actions beside the bubble: react stays adjacent to the
    // bubble, reply sits on the outer edge, evenly spaced from react and the
    // bubble.
    final hoverReactButton = GlassCircleButton(
      key: reactButtonKey,
      size: _hoverReactSize,
      color: isSender
          ? colors.message.selfBackground
          : colors.message.otherBackground,
      icon: AppIcon.smilePlus(size: 18, color: colors.text.secondary),
      onPressed: () => openReactionMenu(anchorKey: reactButtonKey),
      shadows: const [],
    );
    final hoverReplyButton = GlassCircleButton(
      size: _hoverReactSize,
      color: isSender
          ? colors.message.selfBackground
          : colors.message.otherBackground,
      icon: AppIcon.cornerLeft(size: 18, color: colors.text.secondary),
      onPressed: () {
        context.read<ChatDetailsCubit>().replyToMessage(messageId: messageId);
      },
      shadows: const [],
    );

    // Padded at the bottom by the chip reserve so the trailing-affordance Row
    // centers it on the bubble itself (not the bubble + reaction chips).
    final hoverAffordance = Padding(
      padding: EdgeInsets.only(
        bottom: reactionsReservedBelow(context, reactions.isNotEmpty),
        left: isSender ? 0 : Spacing.px8,
        right: isSender ? Spacing.px8 : 0,
      ),
      child: ValueListenableBuilder(
        valueListenable: isHovered,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: isSender
              ? [
                  hoverReplyButton,
                  const SizedBox(width: Spacing.px8),
                  hoverReactButton,
                ]
              : [
                  hoverReactButton,
                  const SizedBox(width: Spacing.px8),
                  hoverReplyButton,
                ],
        ),
        builder: (context, hovered, child) => AnimatedOpacity(
          opacity: hovered ? 1.0 : 0.0,
          duration: const Duration(milliseconds: 120),
          child: IgnorePointer(ignoring: !hovered, child: child),
        ),
      ),
    );

    final desktopShell = ContextMenu(
      direction: isSender
          ? ContextMenuDirection.left
          : ContextMenuDirection.right,
      offset: const Offset(Spacing.px8, 0),
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
        messageKey: messageBubbleKey,
        detached: false,
        includeMetadata: showMetadata,
        trailingAffordance: isReplyable ? hoverAffordance : null,
      ),
    );

    return WrapWithBubbleWidth(
      isSender: isSender,
      affordanceWidth: isReplyable ? _hoverAffordanceWidth : 0,
      child: MouseRegion(
        onEnter: (_) => isHovered.value = true,
        onExit: (_) => isHovered.value = false,
        child: desktopShell,
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
        fileName: p.basename(attachment.filename),
        mimeType: attachment.contentType,
        data: data,
      );
    } else if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
      // On Desktop, we save the attachment in Rust after getting a path from the platform-specific
      // dialog
      final attachmentsRepository = context.read<AttachmentsRepository>();
      final location = await getSaveLocation(
        suggestedName: p.basename(attachment.filename),
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
        showErrorBannerStandalone((loc) => loc.messageContextMenu_saveError);
        return;
      }
    } else if (Platform.isIOS) {
      throw UnsupportedError("iOS does not support storing files");
    } else {
      throw UnsupportedError("Unsupported platform");
    }

    // TODO: Snackbar overlaps with the composer, so we need a better solution
    showSnackBarStandalone(
      (loc) => SnackBar(
        duration: const Duration(seconds: 1),
        content: Text(loc.messageContextMenu_saveConfirmation),
      ),
    );
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

class _MessageMetadataRow extends StatefulWidget {
  const _MessageMetadataRow({
    required this.timestamp,
    required this.isSender,
    required this.flightPosition,
    required this.status,
  });

  final DateTime timestamp;
  final bool isSender;
  final UiFlightPosition flightPosition;
  final UiMessageStatus status;

  @override
  State<_MessageMetadataRow> createState() => _MessageMetadataRowState();
}

class _MessageMetadataRowState extends State<_MessageMetadataRow> {
  Timer? _sendingTimer;
  bool _showSending = false;

  @override
  void initState() {
    super.initState();
    _updateSendingTimer();
  }

  @override
  void didUpdateWidget(_MessageMetadataRow oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.status != widget.status) {
      _updateSendingTimer();
    }
  }

  void _updateSendingTimer() {
    if (widget.status == UiMessageStatus.sending) {
      if (!_showSending && _sendingTimer == null) {
        _sendingTimer = Timer(const Duration(seconds: 2), () {
          _sendingTimer = null;
          setState(() => _showSending = true);
        });
      }
    } else {
      _sendingTimer?.cancel();
      _sendingTimer = null;
      _showSending = false;
    }
  }

  @override
  void dispose() {
    _sendingTimer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!widget.flightPosition.isLast) {
      return const SizedBox.shrink();
    }

    final loc = AppLocalizations.of(context);
    final showMessageStatus =
        widget.isSender && widget.status != UiMessageStatus.hidden;
    final isError = widget.status == UiMessageStatus.error;
    final isSending = widget.status == UiMessageStatus.sending;
    final showTimestamp = !isError && !(isSending && _showSending);
    final double leadingSpacing = widget.isSender ? Spacing.px16 : 0;

    return SelectionContainer.disabled(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const SizedBox(height: 2),
          // We use this to avoid overflow issues when transitioning between
          // mobile & desktop views,
          FittedBox(
            fit: BoxFit.scaleDown,
            alignment: widget.isSender
                ? Alignment.centerRight
                : Alignment.centerLeft,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                SizedBox(width: leadingSpacing),
                if (showTimestamp) Timestamp(widget.timestamp),
                if (showMessageStatus) const SizedBox(width: Spacing.px4),
                if (showMessageStatus && isError)
                  Text(
                    style: TextStyle(
                      color: CustomColorScheme.of(context).function.warning,
                      fontSize: LabelFontSize.small2.size,
                    ),
                    loc.messageBubble_failedToSend,
                  ),
                if (showMessageStatus && isSending && _showSending)
                  Text(
                    style: TextStyle(
                      color: CustomColorScheme.of(context).text.tertiary,
                      fontSize: LabelFontSize.small2.size,
                    ),
                    loc.messageBubble_sending,
                  ),
                if (showMessageStatus &&
                    (isError || (isSending && _showSending)))
                  const SizedBox(width: Spacing.px4),
                if (showMessageStatus)
                  MessageStatusIndicator(status: widget.status),
                const SizedBox(width: Spacing.px12),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _MessageContent extends StatelessWidget {
  const _MessageContent({
    required this.messageId,
    required this.content,
    required this.inReplyToMessage,
    required this.isSender,
    required this.senderId,
    required this.flightPosition,
    required this.isEdited,
    required this.isHidden,
    required this.isError,
    required this.enableSelection,
  });

  final MessageId messageId;
  final UiMimiContent content;
  final UiInReplyToMessage? inReplyToMessage;
  final bool isSender;
  final UiUserId senderId;
  final UiFlightPosition flightPosition;
  final bool isEdited;
  final bool isHidden;
  final bool isError;
  final bool enableSelection;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    final inReplyTo = inReplyToMessage;
    final bool isReply = inReplyTo != null;
    final bool isDeleted = content.isDeleted;
    final bool isJumboEmoji =
        !isDeleted && !isHidden && !isReply && isJumboEmojiMessage(content);
    // Hide the bubble background and padding for jumbo emoji
    final nakedContent = isJumboEmoji;
    // Adjust padding when sender label is not shown
    const nakedPadding = EdgeInsets.only(
      left: messageHorizontalPadding,
      top: messageVerticalPadding,
      bottom: messageVerticalPadding,
    );
    final List<Widget> columnChildren = [];

    // For deleted messages, show a placeholder text instead of the actual
    // content.
    if (isDeleted) {
      return _DeletedMessageContent(
        isSender: isSender,
        senderId: senderId,
        flightPosition: flightPosition,
      );
    }

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
                color: colors.text.tertiary,
              ),
            ),
          ),
        ),
      );
    } else {
      final List<Widget> selectableBlocks = [];

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
          (inner) => buildBlockElement(context, inner.element, isSender),
        ),
      );

      if (selectableBlocks.isNotEmpty) {
        final textColumn = Padding(
          padding: nakedContent
              ? nakedPadding.copyWith(bottom: isEdited ? 0 : null)
              : _messagePadding.copyWith(bottom: isEdited ? 0 : null),
          child: Column(
            spacing: BodyFontSize.base.size,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: selectableBlocks,
          ),
        );
        final Widget selectableChild = enableSelection
            // Prevents SelectableRegion from selecting
            // words on right-click.
            ? RawGestureDetector(
                gestures: {
                  _EagerSecondaryClickRecognizer:
                      GestureRecognizerFactoryWithHandlers<
                        _EagerSecondaryClickRecognizer
                      >(_EagerSecondaryClickRecognizer.new, (_) {}),
                },
                child: SelectableRegion(
                  selectionControls: emptyTextSelectionControls,
                  contextMenuBuilder: (context, _) => const SizedBox.shrink(),
                  child: textColumn,
                ),
              )
            : SelectionContainer.disabled(child: textColumn);
        columnChildren.add(selectableChild);
      }
    }

    Widget messageBubble = Column(
      crossAxisAlignment: inReplyTo != null ? .stretch : .start,
      children: [
        if (inReplyTo != null)
          MouseRegion(
            cursor: switch (inReplyTo) {
              UiInReplyToMessage_Resolved(:final mimiContent)
                  when !mimiContent.isDeleted =>
                SystemMouseCursors.click,
              _ => MouseCursor.defer,
            },
            child: GestureDetector(
              onTap: () {
                if (inReplyTo case UiInReplyToMessage_Resolved(
                  :final messageId,
                  :final mimiContent,
                ) when !mimiContent.isDeleted) {
                  context.read<MessageListCubit>().jumpToMessage(
                    messageId: messageId,
                  );
                }
              },
              child: Padding(
                padding: const EdgeInsets.only(
                  left: Spacing.px12,
                  right: Spacing.px12,
                  top: Spacing.px12,
                ),
                child: InReplyToBubble(
                  inReplyTo: inReplyTo,
                  backgroundColor: colors.fill.secondary,
                ),
              ),
            ),
          ),
        ...columnChildren,
      ],
    );

    if (inReplyTo != null) {
      messageBubble = IntrinsicWidth(child: messageBubble);
    }

    final borderRadius = _messageBorderRadius(isSender, flightPosition);
    final baseColor = isSender
        ? colors.message.selfBackground
        : colors.message.otherBackground;
    final bubbleContent = DefaultTextStyle.merge(
      child: Column(
        crossAxisAlignment: .end,
        children: [
          messageBubble,
          if (isEdited)
            Padding(
              padding: const EdgeInsets.only(
                left: Spacing.px16,
                right: Spacing.px16,
                bottom: Spacing.px8,
              ),
              child: SelectionContainer.disabled(
                child: Text(
                  loc.textMessage_edited,
                  style: Theme.of(context).textTheme.bodySmall!.copyWith(
                    color: isSender
                        ? colors.message.selfEditedLabel
                        : colors.message.otherEditedLabel,
                  ),
                ),
              ),
            ),
        ],
      ),
    );

    return Padding(
      padding: const EdgeInsets.only(bottom: 1.5),
      // Jumbo emoji renders without a bubble background, we skip the jump
      // highlight
      child: nakedContent
          ? bubbleContent
          : JumpHighlight(
              id: messageId,
              borderRadius: borderRadius,
              baseColor: baseColor,
              child: bubbleContent,
            ),
    );
  }
}

class _DeletedMessageContent extends StatelessWidget {
  const _DeletedMessageContent({
    required this.isSender,
    required this.senderId,
    required this.flightPosition,
  });

  final bool isSender;
  final UiUserId senderId;
  final UiFlightPosition flightPosition;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final deletedText = isSender
        ? loc.textMessage_deletedBySelf
        : loc.textMessage_deletedByOther(
            context
                .select(
                  (UsersCubit cubit) => cubit.state.profile(userId: senderId),
                )
                .displayName,
          );
    final borderColor = isSender
        ? colors.message.selfBackground
        : colors.message.otherBackground;

    return Padding(
      padding: const EdgeInsets.only(bottom: 1.5),
      child: DecoratedBox(
        decoration: BoxDecoration(
          borderRadius: _messageBorderRadius(isSender, flightPosition),
          border: Border.all(color: borderColor),
        ),
        child: SelectionContainer.disabled(
          child: Padding(
            padding: _messagePadding,
            child: Text(
              deletedText,
              style: TextStyle(
                fontStyle: FontStyle.italic,
                fontSize: BodyFontSize.base.size,
                color: colors.text.tertiary,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _SenderHeader extends StatelessWidget {
  const _SenderHeader({required this.displayName, required this.onTap});

  final String displayName;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: _DisplayName(displayName: displayName),
      ),
    );
  }
}

class _SenderAvatar extends StatelessWidget {
  const _SenderAvatar({
    required this.sender,
    required this.onTap,
    this.size = senderAvatarSize,
  });

  final UiUserId sender;
  final VoidCallback onTap;
  final double size;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: sender),
    );
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: UserAvatar(profile: profile, size: size),
      ),
    );
  }
}

class _DisplayName extends StatelessWidget {
  const _DisplayName({required this.displayName});

  final String displayName;

  @override
  Widget build(BuildContext context) {
    return SelectionContainer.disabled(
      child: Text(
        displayName,
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
    return Hero(
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
            onTap: () {
              FocusScope.of(context).unfocus();
              HapticFeedback.mediumImpact();
              Navigator.of(
                context,
              ).push(imageViewerRoute(attachment: attachment));
            },
          ),
        ),
      ),
    );
  }
}

void _showDeleteMessageDialog({
  required BuildContext context,
  required MessageId messageId,
}) {
  final loc = AppLocalizations.of(context);
  final cubit = context.read<ChatDetailsCubit>();

  showBottomSheetModal(
    context: context,
    builder: (sheetContext) => BottomSheetDialogContent(
      title: loc.deleteMessageDialog_title,
      description: loc.deleteMessageDialog_description,
      primaryActionText: loc.deleteMessageDialog_forEveryone,
      onPrimaryAction: (_) => cubit.deleteMessage(
        messageId: messageId,
        deleteMode: DeleteMode.forEveryone,
      ),
      primaryType: AppButtonType.secondary,
      primaryTone: AppButtonTone.danger,
      secondaryActionText: loc.deleteMessageDialog_forMe,
      onSecondaryAction: (_) => cubit.deleteMessage(
        messageId: messageId,
        deleteMode: DeleteMode.forMe,
      ),
      secondaryType: AppButtonType.secondary,
      secondaryTone: AppButtonTone.danger,
    ),
  );
}

void _showDeleteForMeDialog({
  required BuildContext context,
  required MessageId messageId,
}) {
  final loc = AppLocalizations.of(context);
  final cubit = context.read<ChatDetailsCubit>();

  showBottomSheetModal(
    context: context,
    builder: (sheetContext) => BottomSheetDialogContent(
      title: loc.deleteMessageForMeDialog_title,
      description: loc.deleteMessageForMeDialog_description,
      primaryActionText: loc.deleteMessageForMeDialog_delete,
      onPrimaryAction: (_) => cubit.deleteMessage(
        messageId: messageId,
        deleteMode: DeleteMode.forMe,
      ),
      primaryType: AppButtonType.secondary,
      primaryTone: AppButtonTone.danger,
    ),
  );
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

/// Immediately wins the gesture arena for secondary (right) mouse button
/// clicks, preventing [SelectableRegion] from selecting words on right-click.
/// Ignores primary button events so text selection via left-click still works.
class _EagerSecondaryClickRecognizer extends EagerGestureRecognizer {
  @override
  bool isPointerAllowed(PointerDownEvent event) {
    return event.buttons == kSecondaryMouseButton;
  }
}
