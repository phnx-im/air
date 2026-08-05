// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/system_message/system_message.dart';
import 'package:air/ds/patterns/system_message/system_message_tokens.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'package:air/features/message_list/contact_request_dialog.dart';
import 'package:air/features/message_list/timestamp.dart';

class DisplayMessageTile extends StatelessWidget {
  final UiEventMessage eventMessage;
  final DateTime timestamp;
  const DisplayMessageTile(this.eventMessage, this.timestamp, {super.key});

  @override
  Widget build(BuildContext context) {
    return switch (eventMessage) {
      UiEventMessage_System(field0: final message) => _SystemMessageContent(
        message: message,
        timestamp: timestamp,
      ),
      UiEventMessage_Error(field0: final message) => SystemMessage(
        tokens: SystemMessageTokens.current,
        tone: SystemMessageTone.danger,
        label: message.message,
        timestamp: Timestamp(timestamp),
      ),
    };
  }
}

class _SystemMessageContent extends StatelessWidget {
  const _SystemMessageContent({required this.message, required this.timestamp});

  final UiSystemMessage message;
  final DateTime timestamp;

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
        _request(
          ContactRequestDialog(
            sender: sender,
            source: .targetedMessage(originChatTitle: chatName),
          ),
        ),
      UiSystemMessage_ReceivedHandleConnectionRequest(
        :final sender,
        :final username,
      )
          when !isConfirmed =>
        _request(
          ContactRequestDialog(
            sender: sender,
            source: .username(username: username),
          ),
        ),
      _ => SystemMessage(
        tokens: SystemMessageTokens.current,
        content: buildSystemMessageText(context, message),
        timestamp: Timestamp(timestamp),
      ),
    };
  }

  /// A pending contact request is something to act on rather than an event to
  /// skim past, so it brings its own surface and only borrows the tile's
  /// spacing and timestamp.
  Widget _request(Widget child) => Padding(
    padding: const EdgeInsets.symmetric(vertical: S.s24),
    child: Column(spacing: S.s4, children: [child, Timestamp(timestamp)]),
  );
}

/// Builds the sentence describing [message], with the names and titles it
/// mentions resolved and emphasized.
///
/// The spans carry no base style: [SystemMessage] applies it to whatever it is
/// handed, so only the emphasized runs need one of their own.
TextSpan buildSystemMessageText(BuildContext context, UiSystemMessage message) {
  final loc = AppLocalizations.of(context);
  final nameStyle = SystemMessage.emphasisOf(
    context,
    SystemMessageVariant.notice,
  );

  String nameOf(UiUserId id) =>
      context.select((UsersCubit c) => c.state.profile(userId: id).displayName);

  return switch (message) {
    UiSystemMessage_Add(field0: final userId, field1: final contactId) =>
      TextSpan(
        children: [
          TextSpan(
            text: loc.systemMessage_userAddedUser_prefix(nameOf(userId)),
            style: nameStyle,
          ),
          TextSpan(text: loc.systemMessage_userAddedUser_infix),
          TextSpan(
            text: loc.systemMessage_userAddedUser_suffix(nameOf(contactId)),
            style: nameStyle,
          ),
        ],
      ),
    UiSystemMessage_Remove(field0: final userId, field1: final contactId) =>
      TextSpan(
        children: [
          TextSpan(
            text: loc.systemMessage_userRemovedUser_prefix(nameOf(userId)),
            style: nameStyle,
          ),
          TextSpan(text: loc.systemMessage_userRemovedUser_infix),
          TextSpan(
            text: loc.systemMessage_userRemovedUser_suffix(nameOf(contactId)),
            style: nameStyle,
          ),
        ],
      ),
    UiSystemMessage_ChangeTitle(
      field0: final userId,
      field1: final oldTitle,
      field2: final newTitle,
    ) =>
      TextSpan(
        children: [
          TextSpan(
            text: loc.systemMessage_userChangedTitle_prefix(nameOf(userId)),
            style: nameStyle,
          ),
          TextSpan(text: loc.systemMessage_userChangedTitle_infix_1),
          TextSpan(
            text: loc.systemMessage_userChangedTitle_infix_2(oldTitle),
            style: nameStyle,
          ),
          TextSpan(text: loc.systemMessage_userChangedTitle_infix_3),
          TextSpan(
            text: loc.systemMessage_userChangedTitle_suffix(newTitle),
            style: nameStyle,
          ),
        ],
      ),
    UiSystemMessage_ChangePicture(:final field0) => TextSpan(
      children: [
        TextSpan(
          text: loc.systemMessage_userChangedPicture_prefix(nameOf(field0)),
          style: nameStyle,
        ),
        TextSpan(text: loc.systemMessage_userChangedPicture_infix),
      ],
    ),
    UiSystemMessage_CreateGroup(field0: final creatorId) => TextSpan(
      children: [
        TextSpan(
          text: loc.systemMessage_userCreatedGroup_prefix(nameOf(creatorId)),
          style: nameStyle,
        ),
        TextSpan(text: loc.systemMessage_userCreatedGroup_suffix),
      ],
    ),
    UiSystemMessage_NewHandleConnectionChat(:final field0) => TextSpan(
      text: loc.systemMessage_newHandleConnectionChat(field0.plaintext),
    ),
    UiSystemMessage_AcceptedConnectionRequest(:final sender, :final username) =>
      TextSpan(
        text: username == null
            ? loc.systemMessage_acceptedDirectConnectionRequest(nameOf(sender))
            : loc.systemMessage_acceptedHandleConnectionRequest(
                nameOf(sender),
                username.plaintext,
              ),
      ),
    UiSystemMessage_ReceivedConnectionConfirmation(:final sender) => TextSpan(
      text: loc.systemMessage_receivedConnectionConfirmation(nameOf(sender)),
    ),
    UiSystemMessage_ReceivedHandleConnectionRequest(
      :final sender,
      :final username,
    ) =>
      TextSpan(
        text: loc.systemMessage_receivedHandleConnectionRequest(
          nameOf(sender),
          username.plaintext,
        ),
      ),
    UiSystemMessage_ReceivedDirectConnectionRequest(
      :final sender,
      :final chatName,
    ) =>
      TextSpan(
        text: loc.systemMessage_receivedDirectConnectionRequest(
          nameOf(sender),
          chatName,
        ),
      ),
    UiSystemMessage_NewDirectConnectionChat(:final field0) => TextSpan(
      text: loc.systemMessage_newDirectConnectionChat(nameOf(field0)),
    ),
    UiSystemMessage_Onboarded() => TextSpan(text: loc.systemMessage_onboarded),
  };
}
