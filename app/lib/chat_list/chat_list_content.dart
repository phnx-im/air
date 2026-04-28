// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:ui' as ui;

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
import 'package:system_date_time_format/system_date_time_format.dart';

import 'chat_list_cubit.dart';

const _previewLineHeight = 1.28;
final _previewFontSize = BodyFontSize.small1.size;

/// Measures the height of two lines of preview text, caching the result
/// as long as the text direction and scaler remain unchanged.
double _twoLinePreviewHeight(ui.TextDirection direction, TextScaler scaler) {
  if (direction == _cachedDirection && scaler == _cachedScaler) {
    return _cachedTwoLineHeight;
  }
  final tp = TextPainter(
    text: TextSpan(
      text: ' ',
      style: TextStyle(fontSize: _previewFontSize, height: _previewLineHeight),
    ),
    textDirection: direction,
    textScaler: scaler,
    maxLines: 1,
  )..layout();
  _cachedTwoLineHeight = tp.height * 2;
  tp.dispose();
  _cachedDirection = direction;
  _cachedScaler = scaler;
  return _cachedTwoLineHeight;
}

// Cached fields for two-line preview height calculation.
ui.TextDirection? _cachedDirection;
TextScaler? _cachedScaler;
double _cachedTwoLineHeight = 0;

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
    this.topPadding = 0,
    this.bottomPadding = 0,
    this.scrollController,
  });

  final ChatDetailsCubitCreate createChatDetailsCubit;
  final double topPadding;
  final double bottomPadding;
  final ScrollController? scrollController;

  @override
  Widget build(BuildContext context) {
    final chatIds = context.select(
      (ChatListCubit cubit) => cubit.state.chatIds,
    );

    if (chatIds.isEmpty) {
      return const _NoChats();
    }

    return ListView.separated(
      controller: scrollController,
      padding: EdgeInsets.only(top: topPadding, bottom: bottomPadding),
      itemCount: chatIds.length,
      separatorBuilder: (context, index) => Divider(
        height: 1,
        thickness: 1,
        indent: Spacings.s + Spacings.xl + Spacings.xs,
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

    return GestureDetector(
      onTap: () => context.read<NavigationCubit>().openChat(chatId),
      behavior: HitTestBehavior.opaque,
      child: Container(
        padding: const EdgeInsets.fromLTRB(
          Spacings.s,
          Spacings.s,
          Spacings.s,
          Spacings.xs,
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
                    spacing: 0,
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
      spacing: Spacings.xs,
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

    final twoLineHeight = _twoLinePreviewHeight(
      Directionality.of(context),
      MediaQuery.textScalerOf(context),
    );

    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      crossAxisAlignment: .center,
      spacing: Spacings.xs,
      children: [
        if (!isBlocked)
          Expanded(
            child: SizedBox(
              height: twoLineHeight,
              child: Align(
                alignment: Alignment.topLeft,
                child: _LastMessage(chat: chat, ownClientId: ownClientId),
              ),
            ),
          ),
        if (!isBlocked) _TrailingIndicator(ownClientId: ownClientId),
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
        AppIcon.ban(size: 16, color: color),
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

class _TrailingIndicator extends StatelessWidget {
  const _TrailingIndicator({required this.ownClientId});

  final UiUserId ownClientId;

  @override
  Widget build(BuildContext context) {
    final (unreadMessages, lastMessage) = context.select((
      ChatDetailsCubit cubit,
    ) {
      final chat = cubit.state.chat;
      return (chat?.unreadMessages, chat?.lastMessage);
    });

    if (unreadMessages != null && unreadMessages > 0) {
      return _UnreadBadge(count: unreadMessages);
    }

    if (lastMessage == null) return const SizedBox.shrink();

    final lastSender = switch (lastMessage.message) {
      UiMessage_Content(field0: final content) => content.sender,
      _ => null,
    };
    if (lastSender != ownClientId) return const SizedBox.shrink();

    return Padding(
      padding: const EdgeInsets.only(right: Spacings.xxs),
      child: MessageStatusIndicator(status: lastMessage.status),
    );
  }
}

class _UnreadBadge extends StatelessWidget {
  const _UnreadBadge({required this.count});

  final int count;

  @override
  Widget build(BuildContext context) {
    if (count < 1) {
      return const SizedBox.shrink();
    }

    final backgroundColor = CustomColorScheme.of(context).function.toggleBlack;

    final badgeText = count <= 100 ? "$count" : "100+";
    return Container(
      alignment: AlignmentDirectional.center,
      constraints: const BoxConstraints(minHeight: 24, minWidth: 40),
      padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
      decoration: BoxDecoration(
        color: backgroundColor,
        borderRadius: BorderRadius.circular(1000),
      ),
      child: Text(
        badgeText,
        style: TextStyle(
          color: CustomColorScheme.of(context).function.toggleWhite,
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
    final loc = AppLocalizations.of(context);

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
      return Text(
        loc.textMessage_hiddenPlaceholder,
        style: TextStyle(
          fontSize: _previewFontSize,
          height: _previewLineHeight,
          fontStyle: FontStyle.italic,
          color: color.text.tertiary,
        ),
      );
    }

    // === Deleted messages ===
    final isDeleted = switch (lastMessage?.message) {
      UiMessage_Content(field0: final content) =>
        content.content.replaces != null && content.content.content == null,
      _ => false,
    };
    if (isDeleted) {
      return Text(
        loc.textMessage_deleted,
        style: TextStyle(
          fontSize: _previewFontSize,
          height: _previewLineHeight,
          fontStyle: FontStyle.italic,
          color: color.text.tertiary,
        ),
      );
    }

    final readStyle = TextStyle(
      fontSize: _previewFontSize,
      height: _previewLineHeight,
      color: Color.alphaBlend(
        color.text.tertiary,
        ChatListContainer.backgroundColor(context),
      ),
    );
    final unreadStyle = readStyle;
    final draftStyle = readStyle.copyWith(fontStyle: FontStyle.italic);

    final showDraft = !isCurrentChat && draftMessage?.isNotEmpty == true;

    final prefixStyle = showDraft
        ? draftStyle
        : readStyle.copyWith(fontWeight: .bold);

    final suffixStyle = chat.unreadMessages > 0 ? unreadStyle : readStyle;

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

    return Text.rich(
      TextSpan(
        children: [
          TextSpan(text: prefix, style: prefixStyle),
          TextSpan(text: suffix, style: suffixStyle),
        ],
      ),
      maxLines: 2,
      softWrap: true,
      overflow: TextOverflow.ellipsis,
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
  TimestampCategory? _category;
  int? _minutesAgo;
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(
      const Duration(seconds: 5),
      (_) => _refreshTimestamp(),
    );
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _category = classifyTimestamp(widget.chat.lastUsed);
    _minutesAgo = DateTime.now().difference(widget.chat.lastUsed).inMinutes;
    _displayTimestamp = _format(_category!);
  }

  @override
  void didUpdateWidget(covariant _LastUpdated oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.chat.lastUsed != widget.chat.lastUsed) {
      _refreshTimestamp();
    }
  }

  void _refreshTimestamp() {
    final newCategory = classifyTimestamp(widget.chat.lastUsed);
    final newMinutes = DateTime.now()
        .difference(widget.chat.lastUsed)
        .inMinutes;
    final categoryChanged = newCategory != _category;
    final minutesChanged =
        newCategory == TimestampCategory.minutes && newMinutes != _minutesAgo;
    if (!categoryChanged && !minutesChanged) return;
    _category = newCategory;
    _minutesAgo = newMinutes;
    final formatted = _format(newCategory);
    if (formatted != _displayTimestamp) {
      setState(() => _displayTimestamp = formatted);
    }
  }

  String _format(TimestampCategory category) {
    final loc = AppLocalizations.of(context);
    final locale = Localizations.localeOf(context).toString();
    final patterns = SystemDateTimeFormat.of(context);
    final timePattern = patterns.timePattern ?? DateFormat.jm(locale).pattern!;
    final datePattern = patterns.datePattern ?? DateFormat.yMd(locale).pattern!;
    return formatTimestamp(
      widget.chat.lastUsed,
      loc,
      category,
      timePattern: timePattern,
      datePattern: datePattern,
      locale: locale,
    );
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Baseline(
      baseline: Spacings.xs,
      baselineType: TextBaseline.alphabetic,
      child: Text(
        _displayTimestamp,
        style: TextStyle(
          color: CustomColorScheme.of(context).text.tertiary,
          fontSize: LabelFontSize.small3.size,
          height: 1.0,
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
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(
          fontSize: LabelFontSize.base.size,
          height: _previewLineHeight,
          fontWeight: FontWeight.bold,
          color: CustomColorScheme.of(context).text.primary,
        ),
      ),
    );
  }
}

enum TimestampCategory {
  now,
  minutes,
  today,
  yesterday,
  thisWeek,
  thisYear,
  older,
}

TimestampCategory classifyTimestamp(DateTime timestamp, {DateTime? now}) {
  now ??= DateTime.now();
  final diff = now.difference(timestamp);

  if (diff.inSeconds < 60) return TimestampCategory.now;
  if (diff.inMinutes < 60) return TimestampCategory.minutes;
  if (now.year == timestamp.year &&
      now.month == timestamp.month &&
      now.day == timestamp.day) {
    return TimestampCategory.today;
  }
  final yesterday = DateTime(now.year, now.month, now.day - 1);
  if (timestamp.year == yesterday.year &&
      timestamp.month == yesterday.month &&
      timestamp.day == yesterday.day) {
    return TimestampCategory.yesterday;
  }
  if (diff.inDays < 7) return TimestampCategory.thisWeek;
  if (now.year == timestamp.year) return TimestampCategory.thisYear;
  return TimestampCategory.older;
}

String formatTimestamp(
  DateTime timestamp,
  AppLocalizations loc,
  TimestampCategory category, {
  required String timePattern,
  required String datePattern,
  required String locale,
  DateTime? now,
}) {
  now ??= DateTime.now();
  return switch (category) {
    TimestampCategory.now => loc.timestamp_now,
    TimestampCategory.minutes => loc.timestamp_minutesAgo(
      now.difference(timestamp).inMinutes,
    ),
    TimestampCategory.today => DateFormat(timePattern).format(timestamp),
    TimestampCategory.yesterday => loc.timestamp_yesterday,
    TimestampCategory.thisWeek => DateFormat.E(locale).format(timestamp),
    TimestampCategory.thisYear => DateFormat(
      _stripYear(datePattern),
    ).format(timestamp),
    TimestampCategory.older => DateFormat(datePattern).format(timestamp),
  };
}

/// Removes year tokens (y, Y) and surrounding separators from a
/// date pattern, e.g. "M/d/yy" → "M/d", "dd.MM.yyyy" → "dd.MM".
String _stripYear(String pattern) {
  // Remove year tokens and any adjacent separator (/ . - , or space)
  return pattern.replaceAll(RegExp(r"[/.\-,\s]*[yY]+[/.\-,\s]*"), '').trim();
}
