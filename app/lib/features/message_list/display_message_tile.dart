// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/core/core.dart';
import 'package:air/features/you/linked_devices_cubit.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/system_message/system_message.dart';
import 'package:air/ds/patterns/system_message/system_message_tokens.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/emphasized_text.dart';
import 'package:collection/collection.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

import 'package:air/features/message_list/contact_request_dialog.dart';
import 'package:air/features/message_list/timestamp.dart';
import 'package:uuid/uuid_value.dart';

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

class _SystemMessageContent extends StatefulHookWidget {
  const _SystemMessageContent({required this.message, required this.timestamp});

  final UiSystemMessage message;
  final DateTime timestamp;

  @override
  State<_SystemMessageContent> createState() => _SystemMessageContentState();
}

class _SystemMessageContentState extends State<_SystemMessageContent> {
  /// One recognizer per person
  final Map<UiUserId, TapGestureRecognizer> _profileTaps = {};
  TapGestureRecognizer? _devicesTap;

  @override
  void dispose() {
    for (final recognizer in _profileTaps.values) {
      recognizer.dispose();
    }
    _devicesTap?.dispose();
    super.dispose();
  }

  TapGestureRecognizer _profileTap(UiUserId userId) => _profileTaps.putIfAbsent(
    userId,
    () =>
        TapGestureRecognizer()
          ..onTap = () =>
              context.read<NavigationCubit>().openMemberDetails(userId),
  );

  TapGestureRecognizer _devicesTapRecognizer() =>
      _devicesTap ??= TapGestureRecognizer()
        ..onTap = () {
          context.read<NavigationCubit>()
            ..switchTab(HomeTab.profile)
            ..openYouSection(YouSection.devices);
        };

  @override
  Widget build(BuildContext context) {
    final isConfirmed = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isConfirmed ?? false,
    );

    final ownUserId = context.read<UserCubit>().state.userId;
    GestureRecognizer? profileTap(UiUserId userId) =>
        userId == ownUserId ? null : _profileTap(userId);
    final groupChatTitle = useRequestGroupChatTitle(widget.message);

    return switch (widget.message) {
      UiSystemMessage_ReceivedDirectConnectionRequest(:final sender)
          when !isConfirmed =>
        _request(
          ContactRequestDialog(
            sender: sender,
            source: .targetedMessage(originChatTitle: groupChatTitle),
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
          groupChatTitle: groupChatTitle,
          recognizerFor: profileTap,
          devicesTap: _devicesTapRecognizer,
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

/// The title of the group chat the contact request [message] went through,
/// kept current as that chat changes. Null when [message] is no such request,
/// or when the group chat is not on this device, because it was deleted or has
/// not synced yet.
String? useRequestGroupChatTitle(UiSystemMessage? message) {
  final groupChat = switch (message) {
    UiSystemMessage_ReceivedDirectConnectionRequest(:final groupChat) ||
    UiSystemMessage_ReceivedAdditionalDirectConnectionRequest(
      :final groupChat,
    ) => groupChat,
    UiSystemMessage_SentDirectConnectionRequest(:final originChatId) =>
      UiRequestGroupChat.chat(originChatId),
    _ => null,
  };
  final chatId = switch (groupChat) {
    UiRequestGroupChat_Chat(:final field0) => field0,
    _ => null,
  };
  final context = useContext();
  final changes = useMemoized(
    () => chatId == null
        ? null
        : context.read<ChatsRepository>().watchChat(chatId),
    [chatId],
  );
  // The stream only triggers the rebuild. We read the chat from the repository,
  // which holds the change before the stream reports it and also covers the
  // build before the stream's first event.
  useStream(changes);
  final chat = chatId == null
      ? null
      : context.read<ChatsRepository>().getChat(chatId);
  return switch (groupChat) {
    UiRequestGroupChat_Title(:final field0) => field0,
    UiRequestGroupChat_Chat() => switch (chat?.chatType) {
      UiChatType_Group(field0: final attributes) => attributes.title,
      _ => null,
    },
    null => null,
  };
}

/// Builds the sentence describing [message], with the names and titles it
/// mentions resolved and emphasized.
///
/// The spans carry no base style: [SystemMessage] applies it to whatever it is
/// handed, so only the emphasized runs need one of their own.
///
/// [groupChatTitle] names the group chat of a contact request, from
/// [useRequestGroupChatTitle].
///
/// [recognizerFor] makes the people the sentence names tappable, and
/// [devicesTap] the link to the device list. A caller that leaves them out,
/// such as the chat list reading the sentence back as plain text, gets the
/// same words with nothing attached.
TextSpan buildSystemMessageText(
  BuildContext context,
  UiSystemMessage message, {
  required String? groupChatTitle,
  GestureRecognizer? Function(UiUserId userId)? recognizerFor,
  GestureRecognizer Function()? devicesTap,
}) {
  final loc = AppLocalizations.of(context);
  final nameStyle = SystemMessage.emphasisOf(
    context,
    SystemMessageVariant.notice,
  );

  String nameOf(UiUserId id) =>
      context.select((UsersCubit c) => c.state.profile(userId: id).displayName);

  String? deviceNameOf(UuidValue clientId) => context.select(
    (LinkedDevicesCubit c) => c.state.devices
        .firstWhereOrNull((device) => device.clientId == clientId)
        ?.name,
  );

  EmphasizedValue user(UiUserId id) =>
      EmphasizedValue(nameOf(id), recognizer: recognizerFor?.call(id));

  EmphasizedValue tapToViewDevices() => EmphasizedValue(
    loc.systemMessage_devicesTapToView,
    recognizer: devicesTap?.call(),
  );

  return switch (message) {
    UiSystemMessage_Add(field0: final adder, field1: final added) =>
      adder == null
          ? emphasizedText(
              (marks) => loc.systemMessage_userWasAdded(marks[0]),
              [user(added)],
              nameStyle,
            )
          : emphasizedText(
              (marks) => loc.systemMessage_userAddedUser(marks[0], marks[1]),
              [user(adder), user(added)],
              nameStyle,
            ),
    UiSystemMessage_Remove(field0: final remover, field1: final removed) =>
      remover == null
          ? emphasizedText(
              (marks) => loc.systemMessage_userWasRemoved(marks[0]),
              [user(removed)],
              nameStyle,
            )
          : emphasizedText(
              (marks) => loc.systemMessage_userRemovedUser(marks[0], marks[1]),
              [user(remover), user(removed)],
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
    UiSystemMessage_ReceivedDirectConnectionRequest(:final sender) => TextSpan(
      text: switch (groupChatTitle) {
        final String title => loc.systemMessage_receivedDirectConnectionRequest(
          nameOf(sender),
          title,
        ),
        null => loc.systemMessage_receivedDirectConnectionRequestUnknownGroup(
          nameOf(sender),
        ),
      },
    ),
    UiSystemMessage_NewDirectConnectionChat(:final field0) => TextSpan(
      text: loc.systemMessage_newDirectConnectionChat(nameOf(field0)),
    ),
    UiSystemMessage_SentDirectConnectionRequest(:final recipient) => TextSpan(
      text: switch (groupChatTitle) {
        final String title => loc.systemMessage_sentDirectConnectionRequest(
          nameOf(recipient),
          title,
        ),
        null => loc.systemMessage_sentDirectConnectionRequestUnknownGroup(
          nameOf(recipient),
        ),
      },
    ),
    UiSystemMessage_ReceivedAdditionalUsernameConnectionRequest(
      :final sender,
      :final username,
    ) =>
      TextSpan(
        text: loc.systemMessage_receivedAdditionalUsernameConnectionRequest(
          nameOf(sender),
          username.plaintext,
        ),
      ),
    UiSystemMessage_ReceivedAdditionalDirectConnectionRequest(:final sender) =>
      TextSpan(
        text: switch (groupChatTitle) {
          final String title =>
            loc.systemMessage_receivedAdditionalDirectConnectionRequest(
              nameOf(sender),
              title,
            ),
          null =>
            loc.systemMessage_receivedAdditionalDirectConnectionRequestUnknownGroup(
              nameOf(sender),
            ),
        },
      ),
    UiSystemMessage_Onboarded() => TextSpan(text: loc.systemMessage_onboarded),
    UiSystemMessage_DeviceLinked(:final field0) => switch (deviceNameOf(
      field0,
    )) {
      final String deviceName => emphasizedText(
        (marks) => loc.systemMessage_deviceLinked(marks[0], marks[1]),
        [EmphasizedValue(deviceName), tapToViewDevices()],
        nameStyle,
      ),
      null => emphasizedText(
        (marks) => loc.systemMessage_deviceLinkedUnknown(marks[0]),
        [tapToViewDevices()],
        nameStyle,
      ),
    },
    UiSystemMessage_DeviceUnlinked(:final field0) => switch (deviceNameOf(
      field0,
    )) {
      final String deviceName => emphasizedText(
        (marks) => loc.systemMessage_deviceUnlinked(marks[0], marks[1]),
        [EmphasizedValue(deviceName), tapToViewDevices()],
        nameStyle,
      ),
      null => emphasizedText(
        (marks) => loc.systemMessage_deviceUnlinkedUnknown(marks[0]),
        [tapToViewDevices()],
        nameStyle,
      ),
    },
    UiSystemMessage_SelfChatCreated() => TextSpan(
      text: loc.systemMessage_selfChatCreated,
    ),
  };
}
