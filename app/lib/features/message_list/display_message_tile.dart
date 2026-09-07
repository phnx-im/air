// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/system_message/system_message.dart';
import 'package:air/ds/patterns/system_message/system_message_tokens.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/emphasized_text.dart';
import 'package:flutter/gestures.dart';
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

class _SystemMessageContent extends StatefulWidget {
  const _SystemMessageContent({required this.message, required this.timestamp});

  final UiSystemMessage message;
  final DateTime timestamp;

  @override
  State<_SystemMessageContent> createState() => _SystemMessageContentState();
}

class _SystemMessageContentState extends State<_SystemMessageContent> {
  /// One recognizer per person
  final Map<UiUserId, TapGestureRecognizer> _profileTaps = {};

  @override
  void dispose() {
    for (final recognizer in _profileTaps.values) {
      recognizer.dispose();
    }
    super.dispose();
  }

  TapGestureRecognizer _profileTap(UiUserId userId) => _profileTaps.putIfAbsent(
    userId,
    () =>
        TapGestureRecognizer()
          ..onTap = () =>
              context.read<NavigationCubit>().openMemberDetails(userId),
  );

  @override
  Widget build(BuildContext context) {
    final isConfirmed = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isConfirmed ?? false,
    );

    final ownUserId = context.read<UserCubit>().state.userId;
    GestureRecognizer? profileTap(UiUserId userId) =>
        userId == ownUserId ? null : _profileTap(userId);

    return switch (widget.message) {
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
        content: buildSystemMessageText(
          context,
          widget.message,
          recognizerFor: profileTap,
        ),
        timestamp: Timestamp(widget.timestamp),
      ),
    };
  }

  /// A pending contact request is something to act on rather than an event to
  /// skim past, so it brings its own surface and only borrows the tile's
  /// spacing and timestamp.
  Widget _request(Widget child) => Padding(
    padding: const EdgeInsets.symmetric(vertical: S.s24),
    child: Column(
      spacing: S.s4,
      children: [child, Timestamp(widget.timestamp)],
    ),
  );
}

/// Builds the sentence describing [message], with the names and titles it
/// mentions resolved and emphasized.
///
/// The spans carry no base style: [SystemMessage] applies it to whatever it is
/// handed, so only the emphasized runs need one of their own.
///
/// [recognizerFor] makes the people the sentence names tappable. A caller that
/// leaves it out, such as the chat list reading the sentence back as plain
/// text, gets the same words with nothing attached.
TextSpan buildSystemMessageText(
  BuildContext context,
  UiSystemMessage message, {
  GestureRecognizer? Function(UiUserId userId)? recognizerFor,
}) {
  final loc = AppLocalizations.of(context);
  final nameStyle = SystemMessage.emphasisOf(
    context,
    SystemMessageVariant.notice,
  );

  String nameOf(UiUserId id) =>
      context.select((UsersCubit c) => c.state.profile(userId: id).displayName);

  EmphasizedValue user(UiUserId id) =>
      EmphasizedValue(nameOf(id), recognizer: recognizerFor?.call(id));

  return switch (message) {
    UiSystemMessage_Add(field0: final userId, field1: final contactId) =>
      emphasizedText(
        (marks) => loc.systemMessage_userAddedUser(marks[0], marks[1]),
        [user(userId), user(contactId)],
        nameStyle,
      ),
    UiSystemMessage_Remove(field0: final userId, field1: final contactId) =>
      emphasizedText(
        (marks) => loc.systemMessage_userRemovedUser(marks[0], marks[1]),
        [user(userId), user(contactId)],
        nameStyle,
      ),
    UiSystemMessage_ChangeTitle(
      field0: final userId,
      field1: final oldTitle,
      field2: final newTitle,
    ) =>
      emphasizedText(
        (marks) =>
            loc.systemMessage_userChangedTitle(marks[0], marks[1], marks[2]),
        [user(userId), EmphasizedValue(oldTitle), EmphasizedValue(newTitle)],
        nameStyle,
      ),
    UiSystemMessage_ChangePicture(:final field0) => emphasizedText(
      (marks) => loc.systemMessage_userChangedPicture(marks[0]),
      [user(field0)],
      nameStyle,
    ),
    UiSystemMessage_CreateGroup(field0: final creatorId) => emphasizedText(
      (marks) => loc.systemMessage_userCreatedGroup(marks[0]),
      [user(creatorId)],
      nameStyle,
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
