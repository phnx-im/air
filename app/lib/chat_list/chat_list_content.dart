// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/chat/chat_details.dart';
import 'package:air/chat_list/chat_list_view.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/message_list/display_message_tile.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:intl/intl.dart';

import 'chat_list_cubit.dart';

typedef ChatDetailsCubitCreate =
    ChatDetailsCubit Function({
      required UserCubit userCubit,
      required UserSettingsCubit userSettingsCubit,
      required ChatId chatId,
      required ChatsRepository chatsRepository,
      required AttachmentsRepository attachmentsRepository,
      bool withMembers,
    });

class ChatListContent extends StatelessWidget {
  const ChatListContent({
    super.key,
    this.createChatDetailsCubit = ChatDetailsCubit.new,
  });

  final ChatDetailsCubitCreate createChatDetailsCubit;

  @override
  Widget build(BuildContext context) {
    final chatIds = context.select(
      (ChatListCubit cubit) => cubit.state.chatIds,
    );

    if (chatIds.isEmpty) {
      return const _NoChats();
    }

    return ListView.separated(
      padding: const EdgeInsets.all(0),
      itemCount: chatIds.length,
      separatorBuilder: (context, index) => Divider(
        height: 1,
        thickness: 1,
        indent: Spacings.xl + Spacings.l,
        color: CustomColorScheme.of(context).separator.secondary,
      ),
      itemBuilder: (BuildContext context, int index) {
        return BlocProvider(
          key: ValueKey(chatIds[index]),
          create: (context) => createChatDetailsCubit(
            userCubit: context.read<UserCubit>(),
            userSettingsCubit: context.read<UserSettingsCubit>(),
            chatId: chatIds[index],
            chatsRepository: context.read<ChatsRepository>(),
            attachmentsRepository: context.read<AttachmentsRepository>(),
            withMembers: false,
          ),
          lazy: false,
          child: _ListTile(chatId: chatIds[index]),
        );
      },
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
      padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
      child: Text(
        loc.chatList_emptyMessage,
        style: TextStyle(color: CustomColorScheme.of(context).text.secondary),
      ),
    );
  }
}

class _ListTile extends StatelessWidget {
  const _ListTile({required this.chatId});

  final ChatId chatId;

  @override
  Widget build(BuildContext context) {
    final currentChatId = context.select(
      (NavigationCubit cubit) => cubit.state.openChatId,
    );
    final isSelected = currentChatId == chatId;

    return ListTile(
      horizontalTitleGap: 0,
      contentPadding: const EdgeInsets.symmetric(horizontal: 0, vertical: 0),
      minVerticalPadding: 0,
      title: Container(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacings.s,
          vertical: Spacings.s,
        ),
        decoration: BoxDecoration(
          color: isSelected
              ? CustomColorScheme.of(context).backgroundElevated.primary
              : null,
        ),
        child: Builder(
          builder: (context) {
            final chat = context.select(
              (ChatDetailsCubit cubit) => cubit.state.chat,
            );
            if (chat == null) {
              return const SizedBox.shrink();
            }
            return Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              crossAxisAlignment: .center,
              spacing: Spacings.xs,
              children: [
                Align(
                  alignment: .centerLeft,
                  child: ChatAvatar(chatId: chat.id, size: 48),
                ),
                Expanded(
                  child: Column(
                    mainAxisSize: .min,
                    crossAxisAlignment: CrossAxisAlignment.center,
                    mainAxisAlignment: MainAxisAlignment.start,
                    spacing: Spacings.xxxs,
                    children: [
                      _ListTileTop(chat: chat),
                      _ListTileBottom(chat: chat),
                    ],
                  ),
                ),
              ],
            );
          },
        ),
      ),
      selected: isSelected,
      onTap: () => context.read<NavigationCubit>().openChat(chatId),
    );
  }
}

class _ListTileTop extends StatelessWidget {
  const _ListTileTop({required this.chat});

  final UiChatDetails chat;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      spacing: Spacings.xxs,
      children: [
        Expanded(child: _ChatTitle(title: chat.title)),
        _LastUpdated(chat: chat),
      ],
    );
  }
}

class _ListTileBottom extends StatelessWidget {
  const _ListTileBottom({required this.chat});

  final UiChatDetails chat;

  @override
  Widget build(BuildContext context) {
    late final UiUserId ownClientId;
    try {
      ownClientId = context.select((UserCubit cubit) => cubit.state.userId);
    } on ProviderNotFoundException {
      return const SizedBox.shrink();
    }
    final isBlocked = chat.status == const UiChatStatus.blocked();

    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      crossAxisAlignment: .center,
      spacing: Spacings.s,
      children: [
        if (!isBlocked)
          Expanded(
            child: Align(
              alignment: Alignment.topLeft,
              child: _LastMessage(chat: chat, ownClientId: ownClientId),
            ),
          ),
        if (!isBlocked)
          Align(
            alignment: Alignment.center,
            child: _UnreadBadge(chatId: chat.id, count: chat.unreadMessages),
          ),
        if (isBlocked)
          const Align(alignment: Alignment.topLeft, child: _BlockedBadge()),
      ],
    );
  }
}

class _BlockedBadge extends StatelessWidget {
  const _BlockedBadge();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final color = CustomColorScheme.of(context).text.tertiary;
    return Row(
      children: [
        AppIcon(type: AppIconType.ban, size: 16, color: color),
        const SizedBox(width: Spacings.xxxs),
        Text(
          loc.chatList_blocked,
          style: TextStyle(
            fontSize: BodyFontSize.small2.size,
            fontStyle: FontStyle.italic,
            color: color,
          ),
        ),
      ],
    );
  }
}

class _UnreadBadge extends StatelessWidget {
  const _UnreadBadge({required this.chatId, required this.count});

  final ChatId chatId;
  final int count;

  @override
  Widget build(BuildContext context) {
    if (count < 1) {
      return const SizedBox.shrink();
    }

    final backgroundColor = CustomColorScheme.of(context).function.toggleWhite;

    final badgeText = count <= 100 ? "$count" : "100+";
    return Container(
      alignment: AlignmentDirectional.center,
      padding: const EdgeInsets.symmetric(horizontal: Spacings.xs, vertical: 6),
      decoration: BoxDecoration(
        color: backgroundColor,
        borderRadius: BorderRadius.circular(1000),
      ),
      child: Text(
        badgeText,
        style: TextStyle(
          color: CustomColorScheme.of(context).function.toggleBlack,
          fontSize: LabelFontSize.small2.size,
          height: 1,
        ),
      ),
    );
  }
}

class _LastMessage extends StatelessWidget {
  const _LastMessage({required this.chat, required this.ownClientId});

  final UiChatDetails chat;
  final UiUserId ownClientId;

  @override
  Widget build(BuildContext context) {
    final isCurrentChat = context.select(
      (NavigationCubit cubit) => cubit.state.chatId == chat.id,
    );

    final color = CustomColorScheme.of(context);

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

    final isHidden = lastMessage?.status == UiMessageStatus.hidden;
    if (isHidden) {
      final loc = AppLocalizations.of(context);
      return Text(
        loc.textMessage_hiddenPlaceholder,
        style: TextStyle(
          fontStyle: FontStyle.italic,
          color: color.text.tertiary,
        ),
      );
    }

    final readStyle = TextStyle(
      fontSize: BodyFontSize.small1.size,
      color: Color.alphaBlend(
        color.text.tertiary,
        ChatListContainer.backgroundColor(context),
      ),
      height: 1.28,
    );
    final unreadStyle = readStyle;
    final draftStyle = readStyle.copyWith(fontStyle: FontStyle.italic);

    final showDraft = !isCurrentChat && draftMessage?.isNotEmpty == true;

    final prefixStyle = showDraft
        ? draftStyle
        : readStyle.copyWith(fontWeight: .bold);

    final suffixStyle = chat.unreadMessages > 0 ? unreadStyle : readStyle;

    final loc = AppLocalizations.of(context);

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
              content.content.plainBody?.isNotEmpty == true
                  ? content.content.plainBody
                  : content.content.attachments.isNotEmpty
                  ? content.content.attachments.first.imageMetadata != null
                        ? loc.chatList_imageEmoji
                        : loc.chatList_fileEmoji
                  : '',
            UiMessage_Display(field0: final eventMessage) =>
              switch (eventMessage) {
                UiEventMessage_System(field0: final systemMessage) => () {
                  final richText = buildSystemMessageText(
                    context,
                    systemMessage,
                  );
                  return richText.text.toPlainText();
                }(),
                _ => null,
              },
            _ => null,
          };

    final baseFontSize =
        readStyle.fontSize ?? DefaultTextStyle.of(context).style.fontSize ?? 14;
    final lineHeight = baseFontSize * (readStyle.height ?? 1.0);
    const maxLines = 2;

    return SizedBox(
      height: lineHeight * maxLines,
      child: Text.rich(
        TextSpan(
          children: [
            TextSpan(text: prefix, style: prefixStyle),
            TextSpan(text: suffix, style: suffixStyle),
          ],
        ),
        maxLines: maxLines,
        softWrap: true,
        overflow: TextOverflow.ellipsis,
        strutStyle: StrutStyle(
          forceStrutHeight: true,
          fontSize: baseFontSize,
          height: readStyle.height,
        ),
      ),
    );
  }
}

class _LastUpdated extends StatefulWidget {
  const _LastUpdated({required this.chat});

  final UiChatDetails chat;

  @override
  State<_LastUpdated> createState() => _LastUpdatedState();
}

class _LastUpdatedState extends State<_LastUpdated> {
  String _displayTimestamp = '';
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _displayTimestamp = formatTimestamp(widget.chat.lastUsed);
    _timer = Timer.periodic(const Duration(seconds: 5), (timer) {
      final newDisplayTimestamp = formatTimestamp(widget.chat.lastUsed);
      if (newDisplayTimestamp != _displayTimestamp) {
        setState(() {
          _displayTimestamp = newDisplayTimestamp;
        });
      }
    });
  }

  @override
  void didUpdateWidget(covariant _LastUpdated oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.chat.lastUsed != widget.chat.lastUsed) {
      setState(() {
        _displayTimestamp = formatTimestamp(widget.chat.lastUsed);
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return Baseline(
      baseline: Spacings.xs,
      baselineType: TextBaseline.alphabetic,
      child: Text(
        _localizedTimestamp(_displayTimestamp, loc),
        style: TextStyle(
          color: CustomColorScheme.of(context).text.tertiary,
          fontSize: LabelFontSize.small2.size,
        ),
      ),
    );
  }
}

class _ChatTitle extends StatelessWidget {
  const _ChatTitle({required this.title});

  final String title;

  @override
  Widget build(BuildContext context) {
    return Baseline(
      baseline: Spacings.s,
      baselineType: TextBaseline.alphabetic,
      child: Text(
        title,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(
          fontSize: LabelFontSize.small1.size,
          fontWeight: FontWeight.bold,
          height: 1,
          color: CustomColorScheme.of(context).text.primary,
        ),
      ),
    );
  }
}

String _localizedTimestamp(String original, AppLocalizations loc) =>
    switch (original) {
      'Now' => loc.timestamp_now,
      'Yesterday' => loc.timestamp_yesterday,
      _ => original,
    };

String formatTimestamp(DateTime timestamp, {DateTime? now}) {
  now ??= DateTime.now();

  final difference = now.difference(timestamp);
  final yesterday = DateTime(now.year, now.month, now.day - 1);

  if (difference.inSeconds < 60) {
    return 'Now';
  } else if (difference.inMinutes < 60) {
    return '${difference.inMinutes}m';
  } else if (now.year == timestamp.year &&
      now.month == timestamp.month &&
      now.day == timestamp.day) {
    return DateFormat('HH:mm').format(timestamp);
  } else if (now.year == timestamp.year &&
      timestamp.year == yesterday.year &&
      timestamp.month == yesterday.month &&
      timestamp.day == yesterday.day) {
    return 'Yesterday';
  } else if (difference.inDays < 7) {
    return DateFormat('E').format(timestamp);
  } else if (now.year == timestamp.year) {
    return DateFormat('dd.MM').format(timestamp);
  } else {
    return DateFormat('dd.MM.yy').format(timestamp);
  }
}
