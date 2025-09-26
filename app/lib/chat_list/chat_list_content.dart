// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/ui/typography/monospace.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:provider/provider.dart';

import 'chat_list_cubit.dart';

class ChatListContent extends StatelessWidget {
  const ChatListContent({super.key});

  @override
  Widget build(BuildContext context) {
    final chats = context.select((ChatListCubit cubit) => cubit.state.chats);

    if (chats.isEmpty) {
      return const _NoChats();
    }

    return ListView.builder(
      padding: const EdgeInsets.all(0),
      itemCount: chats.length,
      physics: const BouncingScrollPhysics().applyTo(
        const AlwaysScrollableScrollPhysics(),
      ),
      itemBuilder: (BuildContext context, int index) {
        return _ListTile(chat: chats[index]);
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
  const _ListTile({required this.chat});

  final UiChatDetails chat;

  @override
  Widget build(BuildContext context) {
    final currentChatId = context.select(
      (NavigationCubit cubit) => cubit.state.openChatId,
    );
    final isSelected = currentChatId == chat.id;
    return ListTile(
      horizontalTitleGap: 0,
      contentPadding: const EdgeInsets.symmetric(
        horizontal: Spacings.xxs,
        vertical: Spacings.xxs,
      ),
      minVerticalPadding: 0,
      title: Container(
        alignment: AlignmentDirectional.centerStart,
        height: 70,
        width: 300,
        padding: const EdgeInsets.symmetric(
          horizontal: Spacings.xs,
          vertical: Spacings.xxs,
        ),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(Spacings.s),
          color:
              isSelected
                  ? CustomColorScheme.of(context).backgroundBase.quaternary
                  : null,
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          crossAxisAlignment: CrossAxisAlignment.center,
          spacing: Spacings.s,
          children: [
            UserAvatar(size: 50, image: chat.picture, displayName: chat.title),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.center,
                mainAxisAlignment: MainAxisAlignment.start,
                spacing: Spacings.xxxs,
                children: [
                  _ListTileTop(chat: chat),
                  Expanded(child: _ListTileBottom(chat: chat)),
                ],
              ),
            ),
          ],
        ),
      ),
      selected: isSelected,
      onTap: () => context.read<NavigationCubit>().openChat(chat.id),
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
    final ownClientId = context.select((UserCubit cubit) => cubit.state.userId);
    final isBlocked = chat.status == const UiChatStatus.blocked();

    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      crossAxisAlignment: CrossAxisAlignment.start,
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
        Icon(Icons.block, color: color),
        const SizedBox(width: Spacings.xxxs),
        Text(
          loc.chatList_blocked,
          style: TextStyle(fontStyle: FontStyle.italic, color: color),
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
    final currentChatId = context.select(
      (NavigationCubit cubit) => cubit.state.chatId,
    );

    if (count < 1 || chatId == currentChatId) {
      return const SizedBox();
    }

    final badgeText = count <= 100 ? "$count" : "100+";
    const double badgeSize = 26;
    return Container(
      alignment: AlignmentDirectional.center,
      constraints: const BoxConstraints(minWidth: badgeSize),
      padding: const EdgeInsets.fromLTRB(7, 0, 7, 2),
      height: badgeSize,
      decoration: BoxDecoration(
        color: CustomColorScheme.of(context).backgroundBase.quaternary,
        borderRadius: BorderRadius.circular(badgeSize / 2),
      ),
      child: Text(
        badgeText,
        style: TextStyle(
          color: CustomColorScheme.of(context).text.primary,
          fontSize: LabelFontSize.small2.size,
          fontWeight: FontWeight.bold,
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

    final lastMessage = chat.lastMessage;
    final draftMessage = chat.draft?.message.trim();

    final readStyle = TextStyle(
      color: CustomColorScheme.of(context).text.primary,
      height: 1.2,
    );
    final unreadStyle = readStyle.copyWith(fontWeight: FontWeight.bold);
    final draftStyle = readStyle.copyWith(
      fontStyle: FontStyle.italic,
      color: CustomColorScheme.of(context).text.tertiary,
    );

    final showDraft = !isCurrentChat && draftMessage?.isNotEmpty == true;

    final prefixStyle =
        showDraft
            ? draftStyle
            : readStyle.copyWith(
              fontWeight: FontWeight.normal,
              color: CustomColorScheme.of(context).text.tertiary,
            );

    final suffixStyle =
        isCurrentChat && chat.unreadMessages > 0 ? unreadStyle : readStyle;

    final loc = AppLocalizations.of(context);

    final prefix =
        showDraft
            ? "${loc.chatList_draft}: "
            : switch (lastMessage?.message) {
              UiMessage_Content(field0: final content)
                  when content.sender == ownClientId =>
                "${loc.chatList_you}: ",
              _ => null,
            };

    final suffix =
        showDraft
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
              _ => null,
            };

    return Text.rich(
      maxLines: 1,
      softWrap: true,
      overflow: TextOverflow.ellipsis,
      TextSpan(
        children: [
          TextSpan(text: prefix, style: prefixStyle),
          TextSpan(text: suffix, style: suffixStyle),
        ],
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
          color: CustomColorScheme.of(context).text.quaternary,
          fontSize: LabelFontSize.small1.size,
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
        title.toUpperCase(),
        overflow: TextOverflow.ellipsis,
        style: TextStyle(
          color: CustomColorScheme.of(context).text.tertiary,
          fontFamily: getSystemMonospaceFontFamily(),
          fontSize: LabelFontSize.small2.size,
          letterSpacing: 1,
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

String formatTimestamp(String t, {DateTime? now}) {
  DateTime timestamp;
  try {
    timestamp = DateTime.parse(t);
  } catch (e) {
    return '';
  }

  now ??= DateTime.now();

  now = now.toLocal();

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
