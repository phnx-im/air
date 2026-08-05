// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async' show unawaited;
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/hover_action/hover_action.dart';
import 'package:air/ds/components/hover_action/hover_action_tokens.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/patterns/media_message/media_message.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble_tokens.dart';
import 'package:air/ds/patterns/message_meta/message_meta.dart';
import 'package:air/ds/patterns/message_row/message_row.dart';
import 'package:air/ds/patterns/message_row/message_row_tokens.dart';
import 'package:air/ds/patterns/message_text/message_text.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/ds/patterns/reply_block/reply_block.dart';
import 'package:air/features/attachments/attachment_actions.dart';
import 'package:air/features/attachments/attachment_file.dart';
import 'package:air/features/attachments/attachment_image.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/emoji/emoji_picker.dart';
import 'package:air/features/emoji/emoji_repository.dart';
import 'package:air/features/emoji/jumbo_emoji.dart';
import 'package:air/features/message_list/image_viewer.dart';
import 'package:air/features/message_list/jump_highlight.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/message_list/message_reactions.dart';
import 'package:air/features/message_list/message_renderer.dart';
import 'package:air/features/message_list/message_hover_time.dart';
import 'package:air/features/message_list/mobile_message_actions.dart';
import 'package:air/features/message_list/swipe_to_reply.dart';
import 'package:air/features/message_list/time_reveal.dart';
import 'package:air/features/message_list/timestamp.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/avatar.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/platform/haptics.dart';
import 'package:flutter/gestures.dart'
    show EagerGestureRecognizer, kSecondaryMouseButton;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// One message in a conversation: the row it sits in, the bubble carrying its
/// content, and everything the reader can do with it.
class TextMessageTile extends HookWidget {
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
    required this.isNewest,
    required this.isNewestOwn,
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

  /// The newest message in the chat.
  final bool isNewest;

  /// The newest message the user sent.
  final bool isNewestOwn;

  @override
  Widget build(BuildContext context) {
    final tokens = MessageRowTokens.current;
    // Held here rather than beside the bubble it keys: the time reveal wraps
    // the whole row and lines its label up with the bubble inside it.
    final bubbleKey = useMemoized(() => GlobalKey());
    // An own message needs no avatar column, and a 1:1 chat has nobody to
    // name: the other party is already on screen.
    final withParticipant = showSender && !isSender;
    // The name opens a flight and the avatar closes it: the row foots its
    // avatar column, so an earlier row would leave it stranded mid-flight.
    final withName = withParticipant && flightPosition.isFirst;
    final withAvatar = withParticipant && flightPosition.isLast;
    final profile = withName || withAvatar
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

    final stamp = _stamp(context);

    // Outside the row rather than around the bubble: the timestamp column hangs
    // off the list's trailing edge, which is the row's own edge and not the
    // bubble's.
    return TimeRevealRow(
      timestamp: timestamp,
      bubbleKey: bubbleKey,
      child: MessageRow(
        tokens: tokens,
        outgoing: isSender,
        reserveAvatar: withParticipant,
        avatar: withAvatar && profile != null
            ? AnimatedPadding(
                duration: Effect.duration(MotionPreset.short),
                curve: Effect.easeOutQuart,
                // Tracks the animated reserve in [BubbleWithReactions] so the
                // avatar stays level with the bubble as the chips push it up.
                padding: EdgeInsets.only(
                  bottom: reactionsReservedBelow(context, reactions.isNotEmpty),
                ),
                child: UserAvatar(
                  profile: profile,
                  size: tokens.avatarSize,
                  onPressed: openMemberDetails,
                ),
              )
            : null,
        senderName: withName ? profile?.displayName : null,
        onTapSender: withName && profile != null ? openMemberDetails : null,
        footer: stamp,
        child: _MessageView(
          messageId: messageId,
          contentMessage: contentMessage,
          inReplyToMessage: inReplyToMessage,
          isSender: isSender,
          status: status,
          reactions: reactions,
          ownUserId: ownUserId,
          timestamp: timestamp,
          showsStamp: stamp != null,
          bubbleKey: bubbleKey,
        ),
      ),
    );
  }

  /// The stamp under the bubble, or null where the message carries none.
  ///
  /// The conversation shows its time where the time is the reader's business:
  /// at the end of the chat, on the reader's own last word in it, and on a
  /// message that still wants attention -- an edit, a send in flight, a send
  /// that reached the server but nobody else yet, a send that failed.
  /// Everywhere else the time is a keystroke away, from the hover tooltip or
  /// the drag-to-reveal column, and the rows stay quiet.
  Widget? _stamp(BuildContext context) {
    final isEdited = contentMessage.edited;
    final wantsAttention =
        isSender &&
        switch (status) {
          UiMessageStatus.sending ||
          UiMessageStatus.sent ||
          UiMessageStatus.error => true,
          UiMessageStatus.delivered ||
          UiMessageStatus.read ||
          UiMessageStatus.hidden => false,
        };
    // The reader's own last message keeps its stamp even once someone has
    // replied since, so how far it got stays on screen.
    final isPrimary = isNewest || isNewestOwn;
    if (!isPrimary && !isEdited && !wantsAttention) return null;

    final loc = AppLocalizations.of(context);
    final readReceipts = context.select(
      (UserSettingsCubit cubit) => cubit.state.readReceipts,
    );
    // An edited message elsewhere in the history surfaces its marker without
    // resurfacing a delivery tick the reader had already left behind.
    final delivery = isSender && (isPrimary || wantsAttention)
        ? _deliveryStatus(status, readReceipts: readReceipts)
        : null;

    return _MessageStamp(
      timestamp: timestamp,
      isSelf: isSender,
      status: delivery,
      // Only the states the reader may have to act on are spelled out.
      statusLabel: switch (delivery) {
        MessageDeliveryStatus.failed => loc.messageBubble_failedToSend,
        MessageDeliveryStatus.sending => loc.messageBubble_sending,
        _ => null,
      },
      editedLabel: isEdited ? loc.textMessage_edited : null,
    );
  }
}

/// How far an own message got, as the stamp reports it. A hidden message
/// reports nothing: its delivery is not the reader's business.
MessageDeliveryStatus? _deliveryStatus(
  UiMessageStatus status, {
  required bool readReceipts,
}) => switch (status) {
  UiMessageStatus.sending => MessageDeliveryStatus.sending,
  UiMessageStatus.sent => MessageDeliveryStatus.sent,
  UiMessageStatus.delivered => MessageDeliveryStatus.delivered,
  UiMessageStatus.read =>
    readReceipts ? MessageDeliveryStatus.read : MessageDeliveryStatus.delivered,
  UiMessageStatus.error => MessageDeliveryStatus.failed,
  UiMessageStatus.hidden => null,
};

/// The stamp under a message bubble: the time it was sent, and how far it got.
class _MessageStamp extends StatelessWidget {
  const _MessageStamp({
    required this.timestamp,
    required this.isSelf,
    required this.status,
    required this.statusLabel,
    required this.editedLabel,
  });

  final DateTime timestamp;
  final bool isSelf;
  final MessageDeliveryStatus? status;
  final String? statusLabel;
  final String? editedLabel;

  @override
  Widget build(BuildContext context) {
    return MessageTimestamp(
      timestamp: timestamp,
      builder: (context, label) => MessageMeta(
        timestamp: label,
        isSelf: isSelf,
        status: status,
        statusLabel: statusLabel,
        editedLabel: editedLabel,
        // The row already insets its footer to the bubble's text.
        contentOffset: S.s0,
      ),
    );
  }
}

/// The bubble and everything that acts on it: selection, the context menus,
/// swipe-to-reply, the hover affordances, and the reaction chips.
class _MessageView extends HookWidget {
  const _MessageView({
    required this.messageId,
    required this.contentMessage,
    required this.inReplyToMessage,
    required this.isSender,
    required this.status,
    required this.reactions,
    required this.ownUserId,
    required this.timestamp,
    required this.showsStamp,
    required this.bubbleKey,
  });

  final MessageId messageId;
  final UiContentMessage contentMessage;
  final UiInReplyToMessage? inReplyToMessage;
  final bool isSender;
  final UiMessageStatus status;
  final List<UiReaction> reactions;
  final UiUserId ownUserId;
  final DateTime timestamp;

  /// Whether the row already carries its own stamp, in which case the hover
  /// tooltip has nothing left to tell the reader.
  final bool showsStamp;

  /// Keys the bubble for everything that has to find it: the menus that anchor
  /// on it, and the time reveal around the row.
  final GlobalKey bubbleKey;

  @override
  Widget build(BuildContext context) {
    final isRevealed = useState(false);
    // Where the last right-click landed, kept only so the reaction bar raised
    // from the context menu opens at the pointer rather than on the bubble.
    final cursorPosition = useRef<Offset?>(null);
    final reactButtonKey = useMemoized(() => GlobalKey());
    final isDetached = useState(false);
    // A ValueNotifier (not useState) so hover changes rebuild only the hover
    // affordance via a ValueListenableBuilder, not the whole message tile (and
    // its reaction chips, which re-measure on every build).
    final isHovered = useMemoized(() => ValueNotifier(false));
    useEffect(() {
      return isHovered.dispose;
    }, [isHovered]);

    final platform = Theme.of(context).platform;
    final isMobilePlatform =
        platform == TargetPlatform.android || platform == TargetPlatform.iOS;
    final isDesktopPlatform =
        platform == TargetPlatform.macOS ||
        platform == TargetPlatform.linux ||
        platform == TargetPlatform.windows;

    // Persisted default skin tone applied to reactions and the picker.
    final skinToneIndex = context.select(
      (UserSettingsCubit cubit) => cubit.state.defaultEmojiSkinTone,
    );
    final skinTone = EmojiSkinVariation
        .values[skinToneIndex.clamp(0, EmojiSkinVariation.values.length - 1)];

    final isDeleted = contentMessage.content.isDeleted;
    final isReplyable = !isDeleted && status != UiMessageStatus.error;
    final isHidden = status == UiMessageStatus.hidden && !isRevealed.value;

    return _MessageShell(
      contentMessage: contentMessage,
      inReplyToMessage: inReplyToMessage,
      isSender: isSender,
      commands: _MessageCommands(
        context: context,
        messageId: messageId,
        reactions: reactions,
        ownUserId: ownUserId,
        skinTone: skinTone,
        isMobilePlatform: isMobilePlatform,
        bubbleKey: bubbleKey,
        cursorPosition: cursorPosition,
      ),
      isDeleted: isDeleted,
      isHidden: isHidden,
      isReplyable: isReplyable,
      isMobilePlatform: isMobilePlatform,
      isDesktopPlatform: isDesktopPlatform,
      isRevealed: isRevealed,
      isDetached: isDetached,
      isHovered: isHovered,
      reactButtonKey: reactButtonKey,
      timestamp: timestamp,
      showsStamp: showsStamp,
    );
  }
}

/// The bubble with everything that acts on it hung around it.
///
/// Split from [_MessageView] so the hooks stay at the top of the subtree and
/// the layout below reads as plain widget code.
class _MessageShell extends StatelessWidget {
  const _MessageShell({
    required this.contentMessage,
    required this.inReplyToMessage,
    required this.isSender,
    required this.commands,
    required this.isDeleted,
    required this.isHidden,
    required this.isReplyable,
    required this.isMobilePlatform,
    required this.isDesktopPlatform,
    required this.isRevealed,
    required this.isDetached,
    required this.isHovered,
    required this.reactButtonKey,
    required this.timestamp,
    required this.showsStamp,
  });

  final UiContentMessage contentMessage;
  final UiInReplyToMessage? inReplyToMessage;
  final bool isSender;
  final _MessageCommands commands;
  final bool isDeleted;
  final bool isHidden;
  final bool isReplyable;
  final bool isMobilePlatform;
  final bool isDesktopPlatform;
  final ValueNotifier<bool> isRevealed;
  final ValueNotifier<bool> isDetached;
  final ValueNotifier<bool> isHovered;
  final GlobalKey reactButtonKey;

  final DateTime timestamp;

  /// See [_MessageView.showsStamp].
  final bool showsStamp;

  /// Reveal-on-hover buttons, sized to match a single-line message bubble.
  static final HoverActionTokens _hoverTokens = HoverActionTokens(
    size:
        typeScale.body.regular.lineHeightPx +
        MessageBubbleTokens.padding.vertical,
  );

  bool get _withHoverActions => !isMobilePlatform && isReplyable;

  /// Width the hover buttons take beside the bubble: two of them, the gap
  /// between them, and the gap to the bubble.
  double get _hoverSlot =>
      _withHoverActions ? 2 * (_hoverTokens.size + _hoverTokens.gap) : 0.0;

  @override
  Widget build(BuildContext context) {
    final actions = _messageActions(
      context,
      commands: commands,
      content: contentMessage.content,
      isSender: isSender,
      isDeleted: isDeleted,
      isReplyable: isReplyable,
    );

    return LayoutBuilder(
      builder: (_, constraints) {
        // The row's column already stops short of the far edge. The hover
        // buttons share it with the bubble, so they come off it first.
        final bubbleMaxWidth = constraints.maxWidth - _hoverSlot;
        return isMobilePlatform
            ? _mobileTile(context, actions, bubbleMaxWidth)
            : _desktopTile(context, actions, bubbleMaxWidth);
      },
    );
  }

  Widget _mobileTile(
    BuildContext context,
    List<MessageAction> actions,
    double bubbleMaxWidth,
  ) {
    final tile = _spanRow(
      _unit(
        context,
        bubbleMaxWidth: bubbleMaxWidth,
        onLongPress: actions.isEmpty
            ? null
            : () => _openMobileActions(
                context,
                actions: actions,
                overlayContent: _content(
                  bubbleMaxWidth: bubbleMaxWidth,
                  enableSelection: false,
                ),
              ),
        onSecondaryTapDown: null,
        enableSelection: false,
        detached: isDetached.value,
      ),
    );
    return isReplyable
        ? SwipeToReplyScope(onReply: commands.reply, child: tile)
        : tile;
  }

  Widget _desktopTile(
    BuildContext context,
    List<MessageAction> actions,
    double bubbleMaxWidth,
  ) {
    return MouseRegion(
      onEnter: (_) => isHovered.value = true,
      onExit: (_) => isHovered.value = false,
      child: _spanRow(
        _unit(
          context,
          bubbleMaxWidth: bubbleMaxWidth,
          onLongPress: null,
          onSecondaryTapDown: actions.isEmpty
              ? null
              : (details) => _openContextMenu(context, details, actions),
          enableSelection: isDesktopPlatform,
          detached: false,
          affordance: _withHoverActions ? _hoverActions(context) : null,
        ),
      ),
    );
  }

  void _openContextMenu(
    BuildContext context,
    TapDownDetails details,
    List<MessageAction> actions,
  ) {
    final position = details.globalPosition;
    commands.cursorPosition.value = position;
    unawaited(
      showOverlayMenu(
        context: context,
        // The pointer keeps the menu off its own tip horizontally, and the
        // popup's own anchor gap carries the vertical clearance.
        anchor: Rect.fromLTWH(position.dx + S.s8, position.dy, 0, 0),
        items: _contextMenuItems(
          context,
          commands: commands,
          actions: actions,
          isReplyable: isReplyable,
        ),
        // The menu opens at the pointer, so it has no trigger to slide out of.
        slideDistance: 0,
      ),
    );
  }

  Widget _content({
    required double bubbleMaxWidth,
    required bool enableSelection,
  }) => _MessageContent(
    messageId: commands.messageId,
    content: contentMessage.content,
    inReplyToMessage: inReplyToMessage,
    isSender: isSender,
    senderId: contentMessage.sender,
    isHidden: isHidden,
    enableSelection: enableSelection,
    maxWidth: bubbleMaxWidth,
  );

  /// The bubble, its reaction chips, the gestures on it, and the hover buttons
  /// beside it -- everything that has to line up with the bubble's own box.
  Widget _unit(
    BuildContext context, {
    required double bubbleMaxWidth,
    required VoidCallback? onLongPress,
    required GestureTapDownCallback? onSecondaryTapDown,
    required bool enableSelection,
    required bool detached,
    Widget? affordance,
  }) {
    Widget bubble = KeyedSubtree(
      key: commands.bubbleKey,
      child: _content(
        bubbleMaxWidth: bubbleMaxWidth,
        enableSelection: enableSelection,
      ),
    );
    if (!enableSelection && isReplyable) {
      bubble = SwipeToReplyBubble(
        icon: AppIcon.cornerLeft(
          size: S.s16,
          color: SemanticPalette.of(context).text.secondary,
        ),
        child: bubble,
      );
    }
    bubble = BubbleWithReactions(
      reactions: commands.reactions,
      ownUserId: commands.ownUserId,
      onTap: commands.showReactors,
      bubble: bubble,
    );

    final interactive = MouseRegion(
      cursor: SystemMouseCursors.basic,
      child: AnimatedOpacity(
        opacity: detached ? Alpha.a0 : Alpha.a100,
        duration: Effect.duration(MotionPreset.short),
        child: IgnorePointer(
          ignoring: detached,
          // Right-click: handled via raw pointer events to bypass the gesture
          // arena (won by _EagerSecondaryClickRecognizer).
          child: Listener(
            onPointerDown: onSecondaryTapDown == null
                ? null
                : (event) {
                    if (event.buttons == kSecondaryMouseButton) {
                      onSecondaryTapDown(
                        TapDownDetails(
                          globalPosition: event.position,
                          localPosition: event.localPosition,
                        ),
                      );
                    }
                  },
            // Tap and long-press: handled via the gesture arena as usual.
            child: GestureDetector(
              behavior: HitTestBehavior.deferToChild,
              onTap: isHidden ? () => isRevealed.value = true : null,
              // Mobile: double-tap a message to react. On desktop, the
              // recognizer must not be registered at all, otherwise it wins the
              // gesture arena and blocks double-click text selection.
              onDoubleTap: isMobilePlatform && isReplyable
                  ? () {
                      AppHaptics.confirm();
                      commands.openReactionMenu();
                    }
                  : null,
              onLongPress: onLongPress,
              child: bubble,
            ),
          ),
        ),
      ),
    );

    if (affordance == null) return interactive;
    return Row(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: isSender
          ? [affordance, interactive]
          : [interactive, affordance],
    );
  }

  /// Spreads [child] over the row's whole content column while keeping it
  /// hugged to its own side, so hovering or swiping beside the bubble still
  /// reaches the message.
  Widget _spanRow(Widget child) => Align(
    alignment: isSender ? Alignment.centerRight : Alignment.centerLeft,
    child: child,
  );

  /// React stays adjacent to the bubble, reply sits on the outer edge, and the
  /// message's time hangs off the end of the two.
  Widget _hoverActions(BuildContext context) {
    final tokens = _hoverTokens;
    final surface = isSender
        ? HoverActionSurface.self
        : HoverActionSurface.other;

    return AnimatedPadding(
      duration: Effect.duration(MotionPreset.short),
      curve: Effect.easeOutQuart,
      // Tracks the animated reserve in [BubbleWithReactions] so the buttons
      // stay centered on the bubble rather than on the bubble plus its chips.
      padding: EdgeInsets.only(
        bottom: reactionsReservedBelow(context, commands.reactions.isNotEmpty),
        left: isSender ? S.s0 : tokens.gap,
        right: isSender ? tokens.gap : S.s0,
      ),
      child: ValueListenableBuilder<bool>(
        valueListenable: isHovered,
        builder: (context, hovered, _) {
          final react = HoverAction(
            key: reactButtonKey,
            tokens: tokens,
            icon: AppIconType.smilePlus,
            surface: surface,
            revealed: hovered,
            onPressed: () => commands.openReactionMenu(anchor: reactButtonKey),
          );
          final reply = HoverAction(
            tokens: tokens,
            icon: AppIconType.cornerLeft,
            surface: surface,
            revealed: hovered,
            onPressed: commands.reply,
          );
          final buttons = Row(
            mainAxisSize: MainAxisSize.min,
            spacing: tokens.gap,
            children: isSender ? [reply, react] : [react, reply],
          );
          // The time keeps its own clearance from the buttons, so it sits
          // outside their row rather than inside its spacing.
          final time = MessageHoverTime(
            timestamp: timestamp,
            hovered: hovered,
            isSelf: isSender,
            enabled: !showsStamp,
          );
          return Row(
            mainAxisSize: MainAxisSize.min,
            children: isSender ? [time, buttons] : [buttons, time],
          );
        },
      ),
    );
  }

  void _openMobileActions(
    BuildContext context, {
    required List<MessageAction> actions,
    required Widget overlayContent,
  }) {
    final anchorRect = commands.bubbleRect();
    if (anchorRect == null) return;
    AppHaptics.menuOpen();
    isDetached.value = true;
    final future = showMobileMessageActions(
      context: context,
      anchorRect: anchorRect,
      actions: actions,
      messageContent: overlayContent,
      alignEnd: isSender,
      reactionSkinTone: commands.skinTone,
      onReact: isReplyable ? commands.sendReaction : null,
      onReactMore: isReplyable
          ? () => unawaited(commands.openFullEmojiPicker())
          : null,
    );
    unawaited(future.whenComplete(() => isDetached.value = false));
  }
}

/// Everything a message tile can do, bound to one message.
///
/// The tile hands these to the gestures, the menus, and the hover buttons
/// alike, so a reply raised from a swipe and one raised from the context menu
/// go through the same call.
class _MessageCommands {
  _MessageCommands({
    required this.context,
    required this.messageId,
    required this.reactions,
    required this.ownUserId,
    required this.skinTone,
    required this.isMobilePlatform,
    required this.bubbleKey,
    required this.cursorPosition,
  });

  final BuildContext context;
  final MessageId messageId;
  final List<UiReaction> reactions;
  final UiUserId ownUserId;
  final EmojiSkinVariation skinTone;
  final bool isMobilePlatform;
  final GlobalKey bubbleKey;

  /// Where the last right-click landed, so a reaction bar raised from the
  /// context menu opens at the pointer.
  final ObjectRef<Offset?> cursorPosition;

  Rect? bubbleRect() => _globalRectOf(bubbleKey);

  static Rect? _globalRectOf(GlobalKey key) {
    final renderObject = key.currentContext?.findRenderObject();
    if (renderObject is! RenderBox || !renderObject.hasSize) return null;
    return renderObject.localToGlobal(Offset.zero) & renderObject.size;
  }

  void reply() {
    context.read<ChatDetailsCubit>().replyToMessage(messageId: messageId);
  }

  void edit() {
    context.read<ChatDetailsCubit>().editMessage(messageId: messageId);
  }

  void sendReaction(String emoji) {
    // May run after the reaction overlay's exit transition.
    if (!context.mounted) return;
    context.read<ChatDetailsCubit>().sendReaction(
      messageId: messageId,
      emoji: emoji,
    );
  }

  /// Pass [barrierColor] transparent when the picker opens on top of the
  /// quick-reaction bar, whose barrier stays alive underneath so the dim
  /// doesn't flicker.
  Future<void> openFullEmojiPicker({Color? barrierColor}) async {
    // Capture the cubit before the await so the picker can persist tone
    // changes.
    final settings = context.read<UserSettingsCubit>();
    void onSkinToneChanged(EmojiSkinVariation tone) {
      unawaited(settings.setDefaultEmojiSkinTone(value: tone.index));
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

  /// Anchors the bar to whatever raised it so it opens centered above it: the
  /// hover react button, else the right-click point, else the bubble as a last
  /// resort (a mobile double-tap raises no anchor of its own).
  void openReactionMenu({GlobalKey? anchor}) {
    final cursor = cursorPosition.value;
    final anchorRect =
        (anchor != null ? _globalRectOf(anchor) : null) ??
        (cursor != null ? cursor & Size.zero : null) ??
        bubbleRect();
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
        onRemove: (emoji) =>
            chatDetailsCubit.deleteReaction(messageId: messageId, emoji: emoji),
      ),
    );
  }
}

const double _menuIconSize = 16;

List<MessageAction> _messageActions(
  BuildContext context, {
  required _MessageCommands commands,
  required UiMimiContent content,
  required bool isSender,
  required bool isDeleted,
  required bool isReplyable,
}) {
  final loc = AppLocalizations.of(context);
  final palette = SemanticPalette.of(context);
  final plainBody = content.plainBody?.trim();
  final attachments = content.attachments;
  final messageId = commands.messageId;

  return [
    if (isReplyable)
      MessageAction(
        label: loc.messageContextMenu_reply,
        leading: const AppIcon.cornerLeft(size: _menuIconSize),
        onSelected: commands.reply,
      ),
    if (plainBody != null && plainBody.isNotEmpty)
      MessageAction(
        label: loc.messageContextMenu_copy,
        leading: const AppIcon.copy(size: _menuIconSize),
        onSelected: () => Clipboard.setData(ClipboardData(text: plainBody)),
      ),
    if (isSender && attachments.isEmpty && !isDeleted)
      MessageAction(
        label: loc.messageContextMenu_edit,
        leading: const AppIcon.pencil(size: _menuIconSize),
        onSelected: commands.edit,
      ),
    if (!isDeleted)
      MessageAction(
        label: loc.messageContextMenu_delete,
        leading: AppIcon.trash(
          size: _menuIconSize,
          color: palette.function.danger,
        ),
        isDestructive: true,
        insertSeparatorBefore: true,
        onSelected: () => isSender
            ? _showDeleteMessageDialog(context: context, messageId: messageId)
            : _showDeleteForMeDialog(context: context, messageId: messageId),
      ),
    if (isDeleted)
      MessageAction(
        label: loc.messageContextMenu_delete,
        leading: AppIcon.trash(
          size: _menuIconSize,
          color: palette.function.danger,
        ),
        isDestructive: true,
        onSelected: () =>
            _showDeleteForMeDialog(context: context, messageId: messageId),
      ),
    if (attachments.isNotEmpty && !Platform.isIOS)
      MessageAction(
        label: loc.messageContextMenu_save,
        leading: const AppIcon.download(size: _menuIconSize),
        onSelected: () => unawaited(saveAttachment(context, attachments.first)),
      ),
    if (attachments.isNotEmpty && Platform.isIOS)
      MessageAction(
        label: loc.messageContextMenu_share,
        leading: const AppIcon.share(size: _menuIconSize),
        onSelected: () => unawaited(shareAttachments(context, attachments)),
      ),
  ];
}

List<MenuItem> _contextMenuItems(
  BuildContext context, {
  required _MessageCommands commands,
  required List<MessageAction> actions,
  required bool isReplyable,
}) {
  final loc = AppLocalizations.of(context);
  final items = <MenuItem>[
    if (isReplyable)
      MenuItem(
        label: loc.messageList_reactions_react,
        leading: const AppIcon.smilePlus(size: _menuIconSize),
        onPressed: commands.openReactionMenu,
      ),
  ];
  for (final action in actions) {
    if (action.insertSeparatorBefore) {
      items.add(const MenuItem.separator());
    }
    items.add(
      MenuItem(
        label: action.label,
        leading: action.leading,
        onPressed: action.onSelected,
        destructive: action.isDestructive,
      ),
    );
  }
  return items;
}

/// The bubble itself: the shape from [MessageBubble], the blocks inside it, and
/// the jump highlight that flashes around it.
class _MessageContent extends StatelessWidget {
  const _MessageContent({
    required this.messageId,
    required this.content,
    required this.inReplyToMessage,
    required this.isSender,
    required this.senderId,
    required this.isHidden,
    required this.enableSelection,
    required this.maxWidth,
  });

  final MessageId messageId;
  final UiMimiContent content;
  final UiInReplyToMessage? inReplyToMessage;
  final bool isSender;
  final UiUserId senderId;
  final bool isHidden;
  final bool enableSelection;

  /// Widest the bubble may get. Applied to the content rather than to the
  /// bubble, so the bubble hugs what it carries and the reaction chips and
  /// hover buttons can line up with its edge.
  final double maxWidth;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    if (content.isDeleted) {
      return MessageBubble(
        isSelf: isSender,
        variant: MessageBubbleVariant.outlined,
        child: _capped(
          MessageBubbleTokens.padding,
          _placeholder(context, _deletedBy(context, loc)),
        ),
      );
    }

    if (isHidden) {
      return _highlighted(
        context,
        MessageBubble(
          isSelf: isSender,
          child: _capped(
            MessageBubbleTokens.padding,
            _placeholder(context, loc.textMessage_hiddenPlaceholder),
          ),
        ),
      );
    }

    final inReplyTo = inReplyToMessage;
    final blocks = [
      for (final inner in content.content?.elements ?? [])
        buildBlockElement(context, inner.element, isSender),
    ];

    // An emoji-only body stands on the conversation without a bubble under it,
    // which also means without a jump highlight around it.
    if (inReplyTo == null && isJumboEmojiMessage(content)) {
      return MessageBubble(
        isSelf: isSender,
        variant: MessageBubbleVariant.naked,
        child: _capped(
          MessageBubbleTokens.padding,
          _selectable(_text(blocks, jumbo: true)),
        ),
      );
    }

    final attachment = content.attachments.firstOrNull;
    final imageMetadata = attachment?.imageMetadata;
    final hasMedia = attachment != null && imageMetadata != null;
    // A picture runs to the bubble's edge, so the blocks beside it carry the
    // inset the bubble would otherwise apply to all of them.
    final padding = hasMedia ? EdgeInsets.zero : MessageBubbleTokens.padding;
    Widget inset(Widget child) => hasMedia
        ? Padding(padding: MessageBubbleTokens.padding, child: child)
        : child;

    // Stacked blocks only share a width where they can be measured. A picture
    // sizes itself against the available width and exposes none.
    final stretched = inReplyTo != null && !hasMedia;

    return _highlighted(
      context,
      MessageBubble(
        isSelf: isSender,
        padding: padding,
        intrinsicWidth: stretched,
        child: _capped(
          padding,
          Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: stretched
                ? CrossAxisAlignment.stretch
                : CrossAxisAlignment.start,
            spacing: hasMedia ? S.s0 : S.s8,
            children: [
              if (inReplyTo != null) inset(_reply(context, inReplyTo)),
              if (hasMedia)
                SelectionContainer.disabled(
                  child: _media(context, attachment, imageMetadata),
                ),
              if (attachment != null && imageMetadata == null)
                inset(
                  SelectionContainer.disabled(
                    child: _file(context, attachment),
                  ),
                ),
              if (blocks.isNotEmpty)
                inset(_selectable(_text(blocks, jumbo: false))),
            ],
          ),
        ),
      ),
    );
  }

  /// Caps the content rather than the bubble, so the bubble still hugs what it
  /// carries once the text wraps.
  Widget _capped(EdgeInsets padding, Widget child) => ConstrainedBox(
    constraints: BoxConstraints(
      maxWidth: (maxWidth - padding.horizontal).clamp(0.0, double.infinity),
    ),
    child: child,
  );

  Widget _highlighted(BuildContext context, Widget bubble) {
    final palette = SemanticPalette.of(context);
    return JumpHighlight(
      id: messageId,
      borderRadius: BorderRadius.circular(MessageBubbleTokens.radius),
      baseColor: isSender
          ? palette.message.selfBackground
          : palette.message.otherBackground,
      child: bubble,
    );
  }

  Widget _text(List<Widget> blocks, {required bool jumbo}) =>
      MessageText(isSelf: isSender, blocks: blocks, jumbo: jumbo);

  Widget _placeholder(BuildContext context, String text) =>
      SelectionContainer.disabled(
        child: Text(text, style: MessageText.placeholderStyleOf(context)),
      );

  Widget _selectable(Widget child) {
    if (!enableSelection) return SelectionContainer.disabled(child: child);
    return RawGestureDetector(
      // Prevents SelectableRegion from selecting words on right-click.
      gestures: {
        _EagerSecondaryClickRecognizer:
            GestureRecognizerFactoryWithHandlers<
              _EagerSecondaryClickRecognizer
            >(_EagerSecondaryClickRecognizer.new, (_) {}),
      },
      child: SelectableRegion(
        selectionControls: emptyTextSelectionControls,
        contextMenuBuilder: (context, _) => const SizedBox.shrink(),
        child: child,
      ),
    );
  }

  String _deletedBy(BuildContext context, AppLocalizations loc) {
    if (isSender) return loc.textMessage_deletedBySelf;
    return loc.textMessage_deletedByOther(
      context
          .select((UsersCubit cubit) => cubit.state.profile(userId: senderId))
          .displayName,
    );
  }

  Widget _reply(BuildContext context, UiInReplyToMessage inReplyTo) {
    final quote = quotedMessage(context, inReplyTo);
    final target = switch (inReplyTo) {
      UiInReplyToMessage_Resolved(:final messageId, :final mimiContent)
          when !mimiContent.isDeleted =>
        messageId,
      _ => null,
    };

    return ReplyBlock(
      preview: quote.preview,
      senderName: quote.senderName,
      fill: SemanticPalette.of(context).fill.secondary,
      showJumpIndicator: target != null,
      thumbnail: quotedThumbnail(context, inReplyTo),
      onTap: target == null
          ? null
          : () => context.read<MessageListCubit>().jumpToMessage(
              messageId: target,
            ),
    );
  }

  Widget _file(BuildContext context, UiAttachment attachment) {
    final message = SemanticPalette.of(context).message;
    return AttachmentFile(
      attachment: attachment,
      isSender: isSender,
      color: isSender ? message.selfText : message.otherText,
    );
  }

  Widget _media(
    BuildContext context,
    UiAttachment attachment,
    UiImageMetadata metadata,
  ) {
    return Hero(
      tag: imageViewerHeroTag(attachment),
      transitionOnUserGestures: true,
      child: MediaMessage.builder(
        naturalWidth: metadata.width.toDouble(),
        naturalHeight: metadata.height.toDouble(),
        isSelf: isSender,
        // The attachment drives its own frames -- an animated picture steps
        // them one by one -- and takes the fit the chosen branch needs.
        builder: (fit) => AttachmentImage(
          attachment: attachment,
          imageMetadata: metadata,
          isSender: isSender,
          fit: fit,
          onTap: (thumbnail) {
            FocusScope.of(context).unfocus();
            Navigator.of(context).push(
              imageViewerRoute(
                attachment: attachment,
                metadata: metadata,
                thumbnail: thumbnail,
              ),
            );
          },
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

  showAdaptiveModal(
    context: context,
    builder: (modalContext) => AdaptiveDialogContent(
      title: loc.deleteMessageDialog_title,
      description: loc.deleteMessageDialog_description,
      primaryActionText: loc.deleteMessageDialog_forEveryone,
      onPrimaryAction: (_) {
        AppHaptics.destructive();
        cubit.deleteMessage(
          messageId: messageId,
          deleteMode: DeleteMode.forEveryone,
        );
      },
      primaryType: ButtonType.secondary,
      primaryTone: ButtonTone.danger,
      secondaryActionText: loc.deleteMessageDialog_forMe,
      onSecondaryAction: (_) {
        AppHaptics.destructive();
        cubit.deleteMessage(messageId: messageId, deleteMode: DeleteMode.forMe);
      },
      secondaryType: ButtonType.secondary,
      secondaryTone: ButtonTone.danger,
    ),
  );
}

void _showDeleteForMeDialog({
  required BuildContext context,
  required MessageId messageId,
}) {
  final loc = AppLocalizations.of(context);
  final cubit = context.read<ChatDetailsCubit>();

  showAdaptiveModal(
    context: context,
    builder: (modalContext) => AdaptiveDialogContent(
      title: loc.deleteMessageForMeDialog_title,
      description: loc.deleteMessageForMeDialog_description,
      primaryActionText: loc.deleteMessageForMeDialog_delete,
      onPrimaryAction: (_) {
        AppHaptics.destructive();
        cubit.deleteMessage(messageId: messageId, deleteMode: DeleteMode.forMe);
      },
      primaryType: ButtonType.secondary,
      primaryTone: ButtonTone.danger,
    ),
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
