// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/palette.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/users_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'contact_request_dialog.dart';
import 'timestamp.dart';

class DisplayMessageTile extends StatelessWidget {
  final UiEventMessage eventMessage;
  final DateTime timestamp;
  const DisplayMessageTile(this.eventMessage, this.timestamp, {super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(vertical: Spacings.m),
      child: Column(
        spacing: Spacings.xxxs,
        children: [
          Container(
            child: switch (eventMessage) {
              UiEventMessage_System(field0: final message) =>
                _SystemMessageContent(message: message),
              UiEventMessage_Error(field0: final message) =>
                _ErrorMessageContent(message: message),
            },
          ),
          Timestamp(timestamp),
        ],
      ),
    );
  }
}

class _SystemMessageContent extends StatelessWidget {
  const _SystemMessageContent({required this.message});

  final UiSystemMessage message;

  @override
  Widget build(BuildContext context) {
    final isConfirmed = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isConfirmed ?? false,
    );

    return switch (message) {
      UiSystemMessage_ReceivedDirectConnectionRequest(
        :final sender,
        :final chatName,
      )
          when !isConfirmed =>
        ContactRequestDialog(
          sender: sender,
          source: .targetedMessage(originChatTitle: chatName),
        ),
      UiSystemMessage_ReceivedHandleConnectionRequest(
        :final sender,
        :final userHandle,
      )
          when !isConfirmed =>
        ContactRequestDialog(
          sender: sender,
          source: .handle(handle: userHandle),
        ),
      _ => Center(
        child: Container(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(Spacings.s),
            border: Border.all(
              color: CustomColorScheme.of(context).separator.secondary,
              width: 2,
            ),
          ),
          padding: const EdgeInsets.symmetric(
            horizontal: Spacings.s,
            vertical: Spacings.xs,
          ),
          child: _SystemMessageText(message: message),
        ),
      ),
    };
  }
}

class _SystemMessageText extends StatelessWidget {
  const _SystemMessageText({required this.message});

  final UiSystemMessage message;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final textStyle = TextStyle(
      color: CustomColorScheme.of(context).text.tertiary,
      fontSize: LabelFontSize.small1.size,
    );

    final profileNameStyle = textStyle.copyWith(fontWeight: FontWeight.bold);

    final messageText = switch (message) {
      UiSystemMessage_Add(field0: final userId, field1: final contactId) => () {
        final (user1Name, user2Name) = context.select(
          (UsersCubit c) => (
            c.state.profile(userId: userId).displayName,
            c.state.profile(userId: contactId).displayName,
          ),
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_userAddedUser_prefix(user1Name),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userAddedUser_infix),
              TextSpan(
                text: loc.systemMessage_userAddedUser_suffix(user2Name),
                style: profileNameStyle,
              ),
            ],
          ),
        );
      }(),
      UiSystemMessage_Remove(field0: final userId, field1: final contactId) =>
        () {
          final (user1Name, user2Name) = context.select(
            (UsersCubit c) => (
              c.state.profile(userId: userId).displayName,
              c.state.profile(userId: contactId).displayName,
            ),
          );
          return RichText(
            text: TextSpan(
              style: textStyle,
              children: [
                TextSpan(
                  text: loc.systemMessage_userRemovedUser_prefix(user1Name),
                  style: profileNameStyle,
                ),
                TextSpan(text: loc.systemMessage_userRemovedUser_infix),
                TextSpan(
                  text: loc.systemMessage_userRemovedUser_suffix(user2Name),
                  style: profileNameStyle,
                ),
              ],
            ),
          );
        }(),
      UiSystemMessage_ChangeTitle(
        field0: final userId,
        field1: final oldTitle,
        field2: final newTitle,
      ) =>
        () {
          final userName = context.select(
            (UsersCubit c) => c.state.profile(userId: userId).displayName,
          );
          return RichText(
            text: TextSpan(
              style: textStyle,
              children: [
                TextSpan(
                  text: loc.systemMessage_userChangedTitle_prefix(userName),
                  style: profileNameStyle,
                ),
                TextSpan(text: loc.systemMessage_userChangedTitle_infix_1),
                TextSpan(
                  text: loc.systemMessage_userChangedTitle_infix_2(oldTitle),
                  style: profileNameStyle,
                ),
                TextSpan(text: loc.systemMessage_userChangedTitle_infix_3),
                TextSpan(
                  text: loc.systemMessage_userChangedTitle_suffix(newTitle),
                  style: profileNameStyle,
                ),
              ],
            ),
          );
        }(),
      UiSystemMessage_ChangePicture(:final field0) => () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: field0).displayName,
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_userChangedPicture_prefix(userName),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userChangedPicture_infix),
            ],
          ),
        );
      }(),
      UiSystemMessage_CreateGroup(field0: final creatorId) => () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: creatorId).displayName,
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_userCreatedGroup_prefix(userName),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userCreatedGroup_suffix),
            ],
          ),
        );
      }(),
      UiSystemMessage_NewHandleConnectionChat(:final field0) => () {
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_newHandleConnectionChat(
                  field0.plaintext,
                ),
                style: textStyle,
              ),
            ],
          ),
        );
      }(),
      UiSystemMessage_AcceptedConnectionRequest(
        :final sender,
        :final userHandle,
      ) =>
        () {
          final userName = context.select(
            (UsersCubit c) => c.state.profile(userId: sender).displayName,
          );
          final String text;
          if (userHandle case final handle?) {
            text = loc.systemMessage_acceptedHandleConnectionRequest(
              userName,
              handle.plaintext,
            );
          } else {
            text = loc.systemMessage_acceptedDirectConnectionRequest(userName);
          }
          return RichText(
            text: TextSpan(
              style: textStyle,
              children: [TextSpan(text: text, style: textStyle)],
            ),
          );
        }(),
      UiSystemMessage_ReceivedConnectionConfirmation(:final sender) => () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: sender).displayName,
        );
        final text = loc.systemMessage_receivedConnectionConfirmation(userName);
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [TextSpan(text: text, style: textStyle)],
          ),
        );
      }(),
      UiSystemMessage_ReceivedHandleConnectionRequest(
        :final sender,
        :final userHandle,
      ) =>
        () {
          final userName = context.select(
            (UsersCubit c) => c.state.profile(userId: sender).displayName,
          );
          final text = loc.systemMessage_receivedHandleConnectionRequest(
            userName,
            userHandle.plaintext,
          );
          return RichText(
            text: TextSpan(
              style: textStyle,
              children: [TextSpan(text: text, style: textStyle)],
            ),
          );
        }(),
      UiSystemMessage_ReceivedDirectConnectionRequest(
        :final sender,
        :final chatName,
      ) =>
        () {
          final userName = context.select(
            (UsersCubit c) => c.state.profile(userId: sender).displayName,
          );
          final text = loc.systemMessage_receivedDirectConnectionRequest(
            userName,
            chatName,
          );
          return RichText(
            text: TextSpan(
              style: textStyle,
              children: [TextSpan(text: text, style: textStyle)],
            ),
          );
        }(),
      UiSystemMessage_NewDirectConnectionChat(:final field0) => () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: field0).displayName,
        );
        final text = loc.systemMessage_newDirectConnectionChat(userName);
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [TextSpan(text: text, style: textStyle)],
          ),
        );
      }(),
    };
    return messageText;
  }
}

RichText buildSystemMessageText(BuildContext context, UiSystemMessage message) {
  final loc = AppLocalizations.of(context);

  final textStyle = TextStyle(
    color: CustomColorScheme.of(context).text.tertiary,
    fontSize: LabelFontSize.small1.size,
  );

  final profileNameStyle = textStyle.copyWith(fontWeight: FontWeight.bold);

  final messageText = switch (message) {
    UiSystemMessage_Add(field0: final userId, field1: final contactId) => () {
      final (user1Name, user2Name) = context.select(
        (UsersCubit c) => (
          c.state.profile(userId: userId).displayName,
          c.state.profile(userId: contactId).displayName,
        ),
      );
      return RichText(
        text: TextSpan(
          style: textStyle,
          children: [
            TextSpan(
              text: loc.systemMessage_userAddedUser_prefix(user1Name),
              style: profileNameStyle,
            ),
            TextSpan(text: loc.systemMessage_userAddedUser_infix),
            TextSpan(
              text: loc.systemMessage_userAddedUser_suffix(user2Name),
              style: profileNameStyle,
            ),
          ],
        ),
      );
    }(),
    UiSystemMessage_Remove(field0: final userId, field1: final contactId) =>
      () {
        final (user1Name, user2Name) = context.select(
          (UsersCubit c) => (
            c.state.profile(userId: userId).displayName,
            c.state.profile(userId: contactId).displayName,
          ),
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_userRemovedUser_prefix(user1Name),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userRemovedUser_infix),
              TextSpan(
                text: loc.systemMessage_userRemovedUser_suffix(user2Name),
                style: profileNameStyle,
              ),
            ],
          ),
        );
      }(),
    UiSystemMessage_ChangeTitle(
      field0: final userId,
      field1: final oldTitle,
      field2: final newTitle,
    ) =>
      () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: userId).displayName,
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_userChangedTitle_prefix(userName),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userChangedTitle_infix_1),
              TextSpan(
                text: loc.systemMessage_userChangedTitle_infix_2(oldTitle),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userChangedTitle_infix_3),
              TextSpan(
                text: loc.systemMessage_userChangedTitle_suffix(newTitle),
                style: profileNameStyle,
              ),
            ],
          ),
        );
      }(),
    UiSystemMessage_ChangePicture(:final field0) => () {
      final userName = context.select(
        (UsersCubit c) => c.state.profile(userId: field0).displayName,
      );
      return RichText(
        text: TextSpan(
          style: textStyle,
          children: [
            TextSpan(
              text: loc.systemMessage_userChangedPicture_prefix(userName),
              style: profileNameStyle,
            ),
            TextSpan(text: loc.systemMessage_userChangedPicture_infix),
          ],
        ),
      );
    }(),
    UiSystemMessage_CreateGroup(field0: final creatorId) => () {
      final userName = context.select(
        (UsersCubit c) => c.state.profile(userId: creatorId).displayName,
      );
      return RichText(
        text: TextSpan(
          style: textStyle,
          children: [
            TextSpan(
              text: loc.systemMessage_userCreatedGroup_prefix(userName),
              style: profileNameStyle,
            ),
            TextSpan(text: loc.systemMessage_userCreatedGroup_suffix),
          ],
        ),
      );
    }(),
    UiSystemMessage_NewHandleConnectionChat(:final field0) => () {
      return RichText(
        text: TextSpan(
          style: textStyle,
          children: [
            TextSpan(
              text: loc.systemMessage_newHandleConnectionChat(field0.plaintext),
              style: textStyle,
            ),
          ],
        ),
      );
    }(),
    UiSystemMessage_AcceptedConnectionRequest(
      :final sender,
      :final userHandle,
    ) =>
      () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: sender).displayName,
        );
        final String text;
        if (userHandle case final handle?) {
          text = loc.systemMessage_acceptedHandleConnectionRequest(
            userName,
            handle.plaintext,
          );
        } else {
          text = loc.systemMessage_acceptedDirectConnectionRequest(userName);
        }
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [TextSpan(text: text, style: textStyle)],
          ),
        );
      }(),
    UiSystemMessage_ReceivedConnectionConfirmation(:final sender) => () {
      final userName = context.select(
        (UsersCubit c) => c.state.profile(userId: sender).displayName,
      );
      final text = loc.systemMessage_receivedConnectionConfirmation(userName);
      return RichText(
        text: TextSpan(
          style: textStyle,
          children: [TextSpan(text: text, style: textStyle)],
        ),
      );
    }(),
    UiSystemMessage_ReceivedHandleConnectionRequest(
      :final sender,
      :final userHandle,
    ) =>
      () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: sender).displayName,
        );
        final text = loc.systemMessage_receivedHandleConnectionRequest(
          userName,
          userHandle.plaintext,
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [TextSpan(text: text, style: textStyle)],
          ),
        );
      }(),
    UiSystemMessage_ReceivedDirectConnectionRequest(
      :final sender,
      :final chatName,
    ) =>
      () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: sender).displayName,
        );
        final text = loc.systemMessage_receivedDirectConnectionRequest(
          userName,
          chatName,
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [TextSpan(text: text, style: textStyle)],
          ),
        );
      }(),
    UiSystemMessage_NewDirectConnectionChat(:final field0) => () {
      final userName = context.select(
        (UsersCubit c) => c.state.profile(userId: field0).displayName,
      );
      final text = loc.systemMessage_newDirectConnectionChat(userName);
      return RichText(
        text: TextSpan(
          style: textStyle,
          children: [TextSpan(text: text, style: textStyle)],
        ),
      );
    }(),
  };
  return messageText;
}

class _ErrorMessageContent extends StatelessWidget {
  const _ErrorMessageContent({required this.message});

  final UiErrorMessage message;

  @override
  Widget build(BuildContext context) {
    return Container(
      alignment: AlignmentDirectional.topStart,
      child: Text(
        message.message,
        style: TextStyle(
          color: AppColors.red,
          fontSize: LabelFontSize.small2.size,
          height: 1.0,
        ),
      ),
    );
  }
}
