// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/chat_list/chat_list_content.dart';
import 'package:air/chat_list/chat_list_cubit.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';

import '../mocks.dart';
import '../helpers.dart';

final userProfiles = [
  UiUserProfile(userId: 1.userId(), displayName: 'Alice'),
  UiUserProfile(userId: 2.userId(), displayName: 'Bob'),
  UiUserProfile(userId: 3.userId(), displayName: 'Eve'),
  UiUserProfile(userId: 4.userId(), displayName: 'Charlie'),
];

final chats = [
  // A contact
  UiChatDetails(
    id: 1.chatId(),
    status: const UiChatStatus.active(),
    chatType: UiChatType_Connection(userProfiles[1]),
    unreadMessages: 10,
    messagesCount: 10,
    attributes: const UiChatAttributes(title: 'Bob', picture: null),
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: UiChatMessage(
      id: 1.messageId(),
      chatId: 1.chatId(),
      timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
      message: UiMessage_Content(
        UiContentMessage(
          sender: 2.userId(),
          sent: true,
          edited: false,
          content: UiMimiContent(
            plainBody: 'Hello Alice',
            topicId: Uint8List(0),
            content: simpleMessage('Hello Alice'),
            attachments: [],
          ),
        ),
      ),
      position: UiFlightPosition.single,
      status: UiMessageStatus.sent,
    ),
  ),
  // Connection request
  UiChatDetails(
    id: 2.chatId(),
    status: const UiChatStatus.active(),
    chatType: const UiChatType_HandleConnection(
      UiUserHandle(plaintext: 'eve_03'),
    ),
    unreadMessages: 0,
    messagesCount: 10,
    attributes: const UiChatAttributes(title: 'Eve', picture: null),
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: UiChatMessage(
      id: 2.messageId(),
      chatId: 2.chatId(),
      timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
      message: UiMessage_Content(
        UiContentMessage(
          sender: 3.userId(),
          sent: true,
          edited: true,
          content: UiMimiContent(
            plainBody:
                'Hello Alice. This is a long message that should not be truncated but properly split into multiple lines.',
            topicId: Uint8List(0),
            content: simpleMessage(
              'Hello Alice. This is a long message that should not be truncated but properly split into multiple lines.',
            ),
            attachments: [],
          ),
        ),
      ),
      position: UiFlightPosition.single,
      status: UiMessageStatus.sent,
    ),
  ),
  // Group chat
  UiChatDetails(
    id: 3.chatId(),
    status: const UiChatStatus.active(),
    chatType: const UiChatType_Group(),
    unreadMessages: 0,
    messagesCount: 10,
    attributes: const UiChatAttributes(title: 'Group', picture: null),
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: UiChatMessage(
      id: 3.messageId(),
      chatId: 3.chatId(),
      timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
      message: UiMessage_Content(
        UiContentMessage(
          sender: 4.userId(),
          sent: true,
          edited: false,
          content: UiMimiContent(
            plainBody: 'Hello All',
            topicId: Uint8List(0),
            content: simpleMessage('Hello All'),
            attachments: [],
          ),
        ),
      ),
      position: UiFlightPosition.single,
      status: UiMessageStatus.sent,
    ),
  ),
  // Group chat with a draft
  UiChatDetails(
    id: 4.chatId(),
    status: const UiChatStatus.active(),
    chatType: const UiChatType_Group(),
    unreadMessages: 0,
    messagesCount: 10,
    attributes: const UiChatAttributes(title: 'Group', picture: null),
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: UiChatMessage(
      id: 3.messageId(),
      chatId: 3.chatId(),
      timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
      message: UiMessage_Content(
        UiContentMessage(
          sender: 4.userId(),
          sent: true,
          edited: false,
          content: UiMimiContent(
            plainBody: 'Hello All',
            topicId: Uint8List(0),
            content: simpleMessage('Hello All'),
            attachments: [],
          ),
        ),
      ),
      position: UiFlightPosition.single,
      status: UiMessageStatus.sent,
    ),
    draft: MessageDraft(
      message: 'Some draft message',
      editingId: null,
      updatedAt: DateTime.now(),
      isCommitted: true,
    ),
  ),
  // A blocked contact
  UiChatDetails(
    id: 5.chatId(),
    status: const UiChatStatus.blocked(),
    chatType: UiChatType_Connection(userProfiles[3]),
    unreadMessages: 0,
    messagesCount: 10,
    attributes: const UiChatAttributes(title: 'Charlie', picture: null),
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: null,
  ),
];

final chatIds = chats.map((chat) => chat.id).toList();

MessageContent simpleMessage(String msg) {
  return MessageContent(
    elements: [
      RangedBlockElement(
        start: 0,
        end: msg.length,
        element: BlockElement_Paragraph([
          RangedInlineElement(
            start: 0,
            end: msg.length,
            element: InlineElement_Text(msg),
          ),
        ]),
      ),
    ],
  );
}

ChatDetailsCubitCreate createMockChatDetailsCubitFactory(
  List<UiChatDetails> chats,
) => ({
  required UserCubit userCubit,
  required UserSettingsCubit userSettingsCubit,
  required ChatId chatId,
  required ChatsRepository chatsRepository,
  bool withMembers = true,
}) {
  final chat = chats.firstWhere((chat) => chat.id == chatId);
  final state = ChatDetailsState(chat: chat, members: []);
  final cubit = MockChatDetailsCubit();
  when(() => cubit.state).thenReturn(state);
  return cubit;
};

void main() {
  group('ChatListContent', () {
    late MockNavigationCubit navigationCubit;
    late MockChatListCubit chatListCubit;
    late MockUserCubit userCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      chatListCubit = MockChatListCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
    });

    Widget buildSubject({required List<UiChatDetails> chats}) =>
        RepositoryProvider<ChatsRepository>.value(
          value: MockChatsRepository(),
          child: MultiBlocProvider(
            providers: [
              BlocProvider<NavigationCubit>.value(value: navigationCubit),
              BlocProvider<UserCubit>.value(value: userCubit),
              BlocProvider<ChatListCubit>.value(value: chatListCubit),
              BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
            ],
            child: Builder(
              builder: (context) {
                return MaterialApp(
                  debugShowCheckedModeBanner: false,
                  theme: themeData(MediaQuery.platformBrightnessOf(context)),
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  home: Scaffold(
                    body: ChatListContent(
                      createChatDetailsCubit: createMockChatDetailsCubitFactory(
                        chats,
                      ),
                    ),
                  ),
                );
              },
            ),
          ),
        );

    testWidgets('renders correctly when there are no chats', (tester) async {
      when(
        () => chatListCubit.state,
      ).thenReturn(const ChatListState(chatIds: []));

      await tester.pumpWidget(buildSubject(chats: []));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_list_content_empty.png'),
      );
    });

    testWidgets('renders correctly', (tester) async {
      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(
          home: HomeNavigationState(chatOpen: true, chatId: chats[1].id),
        ),
      );

      final testChats = List.generate(
        20,
        (index) => chats[index % chats.length],
      );
      final testChatIds = testChats.map((chat) => chat.id).toList();

      when(
        () => chatListCubit.state,
      ).thenReturn(ChatListState(chatIds: testChatIds));

      await tester.pumpWidget(buildSubject(chats: testChats));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_list_content.png'),
      );
    });
  });
}
