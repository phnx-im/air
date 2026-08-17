// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/mute_chat_sheet.dart';
import 'package:air/ds/components/counter/counter.dart';
import 'package:air/ds/components/counter/counter_tokens.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/patterns/chat_list/chat_list.dart';
import 'package:air/ds/patterns/chat_list/chat_list_item.dart';
import 'package:air/ds/patterns/chat_list/chat_list_item_tokens.dart';
import 'package:air/ds/patterns/chat_list/chat_list_status_indicator.dart';
import 'package:air/ds/patterns/chat_list/chat_list_timestamp.dart';
import 'package:air/ds/patterns/chat_list/chat_list_tokens.dart';
import 'package:air/ds/patterns/message_meta/message_meta.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/features/message_list/display_message_tile.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/platform/haptics.dart';
import 'package:air/features/user/avatar.dart';
import 'package:air/util/time/app_clock.dart';
import 'package:air/util/time/time_labels.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'package:air/features/chat_list/chat_list_cubit.dart';

typedef ChatDetailsCubitCreate =
    ChatDetailsCubit Function({
      required UserCubit userCubit,
      required UserSettingsCubit userSettingsCubit,
      required ChatId chatId,
      required ChatsRepository chatsRepository,
      required AttachmentsRepository attachmentsRepository,
      bool withMembers,
    });

/// The surface the list paints on: the surrounding panel in the two-pane
/// layout, or its own background tier when it fills the screen.
Color chatListBackgroundColor(BuildContext context) =>
    PanelSurface.maybeOf(context) ??
    SemanticPalette.of(context).backgroundBase.primary;

/// The chats, resolved from [ChatListCubit] and handed to the design system's
/// list: each row gets its own [ChatDetailsCubit], and every piece of localized
/// copy is picked here.
class ChatListContent extends StatelessWidget {
  const ChatListContent({
    super.key,
    this.createChatDetailsCubit = ChatDetailsCubit.new,
    this.header = const SizedBox.shrink(),
    this.headerHeight = 0,
    this.onScrollOffset,
  });

  final ChatDetailsCubitCreate createChatDetailsCubit;

  /// Pinned over the list, which scrolls behind it. It floats over a full-bleed
  /// list, so the host insets it for the status bar itself.
  final Widget header;

  /// What the first row clears, before the list's own header clearance.
  final double headerHeight;

  /// Reports the offset as the list moves, for a header that reveals its title
  /// once rows slide under it.
  final ValueChanged<double>? onScrollOffset;

  @override
  Widget build(BuildContext context) {
    final chatIds = context.select(
      (ChatListCubit cubit) => cubit.state.chatIds,
    );

    final list = ChatList(
      tokens: ChatListTokens.current,
      backgroundColor: chatListBackgroundColor(context),
      header: header,
      headerHeight: headerHeight,
      itemCount: chatIds.length,
      itemBuilder: (context, index) {
        final chatId = chatIds[index];
        final isLast = index == chatIds.length - 1;
        return BlocProvider(
          key: ValueKey(chatId),
          create: (context) => createChatDetailsCubit(
            userCubit: context.read<UserCubit>(),
            userSettingsCubit: context.read<UserSettingsCubit>(),
            chatId: chatId,
            chatsRepository: context.read<ChatsRepository>(),
            attachmentsRepository: context.read<AttachmentsRepository>(),
            withMembers: false,
          ),
          lazy: false,
          child: _ListTile(
            chatId: chatId,
            nextChatId: isLast ? null : chatIds[index + 1],
          ),
        );
      },
      onScrollOffset: onScrollOffset,
    );

    if (chatIds.isNotEmpty) return list;

    // The frame and its header stay, so an account with no chats still opens on
    // the same screen as one with them.
    return Stack(
      children: [
        list,
        const Positioned.fill(child: _NoChats()),
      ],
    );
  }
}

class _NoChats extends StatelessWidget {
  const _NoChats();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return Container(
      alignment: AlignmentDirectional.center,
      padding: const EdgeInsets.symmetric(horizontal: S.s16),
      child: Text(
        loc.chatList_emptyMessage,
        style: TextStyle(color: SemanticPalette.of(context).text.secondary),
      ),
    );
  }
}

class _ListTile extends StatelessWidget {
  const _ListTile({required this.chatId, required this.nextChatId});

  final ChatId chatId;

  /// The row below this one, null on the last row.
  final ChatId? nextChatId;

  @override
  Widget build(BuildContext context) {
    final currentChatId = context.select(
      (NavigationCubit cubit) => cubit.state.openChatId,
    );
    final isActive = currentChatId == chatId;

    // The last row drops its rule so the list does not end on a dangling line.
    // Only the two-pane layout keeps a selection, and there the rules above and
    // below it go too, so the selection reads as one surface.
    final hideSeparator =
        nextChatId == null ||
        (!context.breakpoint.isSmall &&
            (isActive || currentChatId == nextChatId));

    final chat = context.select((ChatDetailsCubit cubit) => cubit.state.chat);
    if (chat == null) {
      return const SizedBox.shrink();
    }

    return _ChatRow(
      chat: chat,
      isActive: isActive,
      hideSeparator: hideSeparator,
      onTap: () => context.read<NavigationCubit>().openChat(chatId),
      onLongPress: (position) => _openMuteMenu(context, position),
    );
  }

  void _openMuteMenu(BuildContext context, Offset position) {
    AppHaptics.menuOpen();
    unawaited(
      showOverlayMenu(
        context: context,
        anchor: Rect.fromLTWH(position.dx, position.dy, 0, 0),
        items: _muteMenuItems(context),
        // The menu opens at the pointer, so it has no trigger to slide out of.
        slideDistance: 0,
      ),
    );
  }

  List<MenuItem> _muteMenuItems(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final cubit = context.read<ChatDetailsCubit>();

    if (cubit.state.chat?.isMuted ?? false) {
      return [
        MenuItem(
          label: loc.chatList_contextMenu_unmute,
          leading: const AppIcon.bell(size: 16),
          onPressed: () => cubit.unmuteChat(),
        ),
      ];
    }

    if (!DeviceType.isDesktop) {
      return [
        MenuItem(
          label: loc.chatList_contextMenu_mute,
          leading: const AppIcon.bellOff(size: 16),
          onPressed: () => showMuteChatSheet(context),
        ),
      ];
    }

    return [
      MenuItem(
        label: loc.chatList_contextMenu_mute,
        leading: const AppIcon.bellOff(size: 16),
        subItems: [
          MenuItem(
            label: loc.muteDurationSheet_1hour,
            onPressed: () =>
                cubit.muteChat(mutedUntil: UiChatMutedExtension.inOneHour()),
          ),
          MenuItem(
            label: loc.muteDurationSheet_8hours,
            onPressed: () =>
                cubit.muteChat(mutedUntil: UiChatMutedExtension.inEightHours()),
          ),
          MenuItem(
            label: loc.muteDurationSheet_untilTomorrow,
            onPressed: () => cubit.muteChat(
              mutedUntil: UiChatMutedExtension.untilTomorrow(),
            ),
          ),
          MenuItem(
            label: loc.muteDurationSheet_untilNextMonday,
            onPressed: () => cubit.muteChat(
              mutedUntil: UiChatMutedExtension.untilNextMonday(),
            ),
          ),
          MenuItem(
            label: loc.muteDurationSheet_always,
            onPressed: () =>
                cubit.muteChat(mutedUntil: const UiChatMuted.forever()),
          ),
        ],
      ),
    ];
  }
}

/// Maps a chat onto the design system's row: it resolves the copy, picks the
/// slots, and leaves the geometry to [ChatListItem].
class _ChatRow extends StatelessWidget {
  const _ChatRow({
    required this.chat,
    required this.isActive,
    required this.hideSeparator,
    required this.onTap,
    required this.onLongPress,
  });

  final UiChatDetails chat;
  final bool isActive;
  final bool hideSeparator;
  final VoidCallback onTap;
  final void Function(Offset position) onLongPress;

  @override
  Widget build(BuildContext context) {
    final tokens = ChatListItemTokens.current;
    final palette = SemanticPalette.of(context);

    UiUserId? ownId;
    try {
      ownId = context.select((UserCubit cubit) => cubit.state.userId);
    } on ProviderNotFoundException {
      ownId = null;
    }

    final isBlocked = chat.status == const UiChatStatus.blocked();

    final Widget? preview;
    if (isBlocked) {
      preview = const _BlockedPreview();
    } else if (ownId != null) {
      preview = _LastMessage(chat: chat, ownClientId: ownId);
    } else {
      preview = null;
    }

    return ChatListItem(
      tokens: tokens,
      title: chat.title,
      avatar: ChatAvatar(chatId: chat.id, size: tokens.avatarSize),
      titleIcon: chat.isMuted
          ? AppIcon.bellOff(
              size: ChatListItemTokens.titleIconSize,
              color: palette.text.tertiary,
            )
          : null,
      timestamp: _LastUpdated(chat: chat),
      preview: preview,
      trailing: isBlocked || ownId == null
          ? null
          : _TrailingIndicator(ownClientId: ownId),
      isActive: isActive,
      hideSeparator: hideSeparator,
      onTap: onTap,
      onLongPress: onLongPress,
    );
  }
}

/// Stands in for the preview on a blocked chat: a ban glyph inline with the
/// label, so it shares the preview's baseline and two-line reserve.
class _BlockedPreview extends StatelessWidget {
  const _BlockedPreview();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final color = SemanticPalette.of(context).text.tertiary;
    return Text.rich(
      TextSpan(
        children: [
          WidgetSpan(
            alignment: PlaceholderAlignment.baseline,
            baseline: TextBaseline.alphabetic,
            child: Padding(
              padding: const EdgeInsets.only(
                right: ChatListItemTokens.previewIconGap,
              ),
              child: AppIcon.ban(
                size: ChatListItemTokens.previewIconSize,
                color: color,
              ),
            ),
          ),
          TextSpan(text: loc.chatList_blocked),
        ],
      ),
      style: typeScale.body.s
          .style(color: color)
          .copyWith(fontStyle: FontStyle.italic),
    );
  }
}

class _TrailingIndicator extends StatelessWidget {
  const _TrailingIndicator({required this.ownClientId});

  final UiUserId ownClientId;

  @override
  Widget build(BuildContext context) {
    final (experimentalFeatures, readReceipts) = context.select(
      (UserSettingsCubit cubit) =>
          (cubit.state.experimentalFeaturesActive, cubit.state.readReceipts),
    );

    final (unreadMessages, lastMessage, pendingCommitFailed) = context.select((
      ChatDetailsCubit cubit,
    ) {
      final chat = cubit.state.chat;
      return (
        chat?.unreadMessages,
        chat?.lastMessage,
        chat?.pendingCommitFailed ?? false,
      );
    });

    if (experimentalFeatures && pendingCommitFailed) {
      return const ChatListStatusIndicator(
        status: MessageDeliveryStatus.failed,
      );
    }

    if (unreadMessages != null && unreadMessages > 0) {
      return Counter(tokens: CounterTokens.current, count: unreadMessages);
    }

    if (lastMessage == null) return const SizedBox.shrink();

    final lastSender = switch (lastMessage.message) {
      UiMessage_Content(field0: final content) => content.sender,
      _ => null,
    };
    if (lastSender != ownClientId) return const SizedBox.shrink();

    final status = _deliveryStatus(
      lastMessage.status,
      readReceipts: readReceipts,
    );
    if (status == null) return const SizedBox.shrink();

    return ChatListStatusIndicator(status: status);
  }
}

/// Null where there is no delivery state to report. A reader with read receipts
/// off must not report back more than the setting does, so a read message stops
/// at delivered. A deleted message reports nothing at all.
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
  UiMessageStatus.hidden || UiMessageStatus.deleted => null,
};

class _LastMessage extends StatelessWidget {
  const _LastMessage({required this.chat, required this.ownClientId});

  final UiChatDetails chat;
  final UiUserId ownClientId;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);

    final previewStyle = typeScale.body.s.style(color: palette.text.tertiary);
    final italicStyle = previewStyle.copyWith(fontStyle: FontStyle.italic);

    final lastMessage = chat.lastMessage;
    final draftMessage = chat.draft?.message.trim();
    final lastSender = switch (lastMessage?.message) {
      UiMessage_Content(field0: final content) => content.sender,
      _ => null,
    };
    final senderDisplayName = lastSender == null
        ? null
        : context.select(
            (UsersCubit cubit) => cubit.state.displayName(userId: lastSender),
          );
    final isGroupChat = chat.chatType is UiChatType_Group;

    // === Hidden messages ===
    final isHidden = lastMessage?.status == UiMessageStatus.hidden;
    if (isHidden) {
      return Text(loc.textMessage_hiddenPlaceholder, style: italicStyle);
    }

    // === Deleted messages ===
    final isDeleted = switch (lastMessage?.message) {
      UiMessage_Content(field0: final content) =>
        content.content.replaces != null && content.content.content == null,
      _ => false,
    };
    if (isDeleted) {
      return Text(loc.textMessage_deleted, style: italicStyle);
    }

    final showDraft = draftMessage?.isNotEmpty == true;

    // === Reactions ===
    final reaction = chat.lastReaction;
    final reactedTo = switch (lastMessage?.message) {
      UiMessage_Content(field0: final content) =>
        content.content.plaintextPreview(loc),
      _ => null,
    };
    if (!showDraft && reaction != null && reactedTo != null) {
      final reactor = reaction.reactor == ownClientId
          ? null
          : context.select(
              (UsersCubit cubit) =>
                  cubit.state.displayName(userId: reaction.reactor),
            );
      return Text(
        reactor == null
            ? loc.chatList_reactionByYou(reaction.emoji, reactedTo)
            : loc.chatList_reaction(reactor, reaction.emoji, reactedTo),
        style: previewStyle,
        maxLines: 2,
        softWrap: true,
        overflow: TextOverflow.ellipsis,
      );
    }

    final prefixStyle = showDraft
        ? italicStyle
        : typeScale.body.s.style(
            color: palette.text.tertiary,
            weight: Weight.emphasized,
          );

    final prefix = showDraft
        ? "${loc.chatList_draft}: "
        : switch (lastSender) {
            final sender when sender == ownClientId => "${loc.chatList_you}: ",
            final sender when sender != null && isGroupChat =>
              senderDisplayName != null ? "$senderDisplayName: " : null,
            _ => null,
          };

    final suffix = showDraft
        ? draftMessage
        : switch (lastMessage?.message) {
            UiMessage_Content(field0: final content) =>
              content.content.plaintextPreview(loc),
            UiMessage_Display(field0: final eventMessage) =>
              switch (eventMessage) {
                UiEventMessage_System(field0: final systemMessage) =>
                  buildSystemMessageText(context, systemMessage).toPlainText(),
                _ => null,
              },
            _ => null,
          };

    return Text.rich(
      TextSpan(
        children: [
          TextSpan(text: prefix, style: prefixStyle),
          TextSpan(text: suffix, style: previewStyle),
        ],
      ),
      maxLines: 2,
      softWrap: true,
      overflow: TextOverflow.ellipsis,
    );
  }
}

/// The "last activity" stamp on a row, kept current by the app clock.
class _LastUpdated extends StatelessWidget {
  const _LastUpdated({required this.chat});

  final UiChatDetails chat;

  @override
  Widget build(BuildContext context) => LiveTime(
    format: (context, now) => chatListStampLabel(
      chat.lastUsed,
      now: now,
      formats: TimeFormats.of(context),
      loc: AppLocalizations.of(context),
    ),
    builder: (context, label) => ChatListTimestamp(label: label),
  );
}
