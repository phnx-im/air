// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/features/chat_list/chat_list_content.dart';
import 'package:air/features/chat_list/chat_list_cubit.dart';
import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:system_date_time_format/system_date_time_format.dart';

import '../../helpers.dart';
import '../../mocks.dart';

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
    isApq: false,
    chatType: UiChatType_Connection(userProfiles[1]),
    unreadMessages: 10,
    messagesCount: 10,
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
      status: UiMessageStatus.sent,
      reactions: [
        UiReaction(emoji: '❤️', users: [1.userId(), 3.userId()]),
        UiReaction(emoji: '🫪', users: [5.userId()]),
      ],
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Connection request
  UiChatDetails(
    id: 2.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: const UiChatType_HandleConnection(
      UiUsername(plaintext: 'eve_03'),
    ),
    unreadMessages: 0,
    messagesCount: 10,
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
      status: UiMessageStatus.sent,
      reactions: [
        UiReaction(emoji: '❤️', users: [1.userId(), 2.userId()]),
      ],
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Group chat
  UiChatDetails(
    id: 3.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: const UiChatType_Group(
      UiChatAttributes(title: 'Group', picture: null),
    ),
    unreadMessages: 0,
    messagesCount: 10,
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
      status: UiMessageStatus.sent,
      reactions: [],
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // Group chat with a draft
  UiChatDetails(
    id: 4.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: const UiChatType_Group(
      UiChatAttributes(title: 'Group', picture: null),
    ),
    unreadMessages: 0,
    messagesCount: 10,
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
      status: UiMessageStatus.sent,
      reactions: [],
    ),
    draft: UiMessageDraft(
      message: 'Some draft message',
      editingId: null,
      updatedAt: DateTime.now(),
      isCommitted: true,
    ),
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // A blocked contact
  UiChatDetails(
    id: 5.chatId(),
    status: const UiChatStatus.blocked(),
    isApq: false,
    chatType: UiChatType_Connection(userProfiles[3]),
    unreadMessages: 0,
    messagesCount: 10,
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: null,
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  // A muted contact
  UiChatDetails(
    id: 6.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(userProfiles[2]),
    unreadMessages: 3,
    messagesCount: 10,
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: UiChatMessage(
      id: 6.messageId(),
      chatId: 6.chatId(),
      timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
      message: UiMessage_Content(
        UiContentMessage(
          sender: 3.userId(),
          sent: false,
          edited: false,
          content: UiMimiContent(
            plainBody: 'Hey, are you there?',
            topicId: Uint8List(0),
            content: simpleMessage('Hey, are you there?'),
            attachments: [],
          ),
        ),
      ),
      status: UiMessageStatus.sent,
      reactions: [],
    ),
    mutedUntil: const UiChatMuted.forever(),
    pendingCommitFailed: false,
  ),
  // Chat where I sent a picture
  UiChatDetails(
    id: 7.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: const UiChatType_Group(
      UiChatAttributes(title: 'Photographs', picture: null),
    ),
    unreadMessages: 1,
    messagesCount: 10,
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: UiChatMessage(
      id: 7.messageId(),
      chatId: 7.chatId(),
      timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
      message: UiMessage_Content(
        UiContentMessage(
          sender: 3.userId(),
          sent: false,
          edited: false,
          content: UiMimiContent(
            topicId: Uint8List(0),
            attachments: [
              UiAttachment(
                attachmentId: 1.attachmentId(),
                filename: "image.webp",
                contentType: "image/webp",
                size: 1024,
              ),
            ],
            firstAttachmentType: .image,
          ),
        ),
      ),
      status: UiMessageStatus.sent,
      reactions: [],
    ),
    pendingCommitFailed: false,
  ),
  // Chat where someone sent a file
  UiChatDetails(
    id: 8.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: const UiChatType_Group(
      UiChatAttributes(title: 'Archive Enthusiasts', picture: null),
    ),
    unreadMessages: 0,
    messagesCount: 10,
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: UiChatMessage(
      id: 8.messageId(),
      chatId: 8.chatId(),
      timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
      message: UiMessage_Content(
        UiContentMessage(
          sender: 1.userId(),
          sent: false,
          edited: false,
          content: UiMimiContent(
            topicId: Uint8List(0),
            attachments: [
              UiAttachment(
                attachmentId: 2.attachmentId(),
                filename: "file.pdf",
                contentType: "application/pdf",
                size: 128,
              ),
            ],
            firstAttachmentType: .file,
          ),
        ),
      ),
      status: UiMessageStatus.sent,
      reactions: [],
    ),
    pendingCommitFailed: false,
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
) =>
    ({
      required UserCubit userCubit,
      required UserSettingsCubit userSettingsCubit,
      required ChatId chatId,
      required ChatsRepository chatsRepository,
      required AttachmentsRepository attachmentsRepository,
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
    late MockUsersCubit usersCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();
      chatListCubit = MockChatListCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(experimentalFeatures: false));
    });

    Widget buildSubject({
      required List<UiChatDetails> chats,
    }) => MultiRepositoryProvider(
      providers: [
        RepositoryProvider<ChatsRepository>.value(value: MockChatsRepository()),
        RepositoryProvider<AttachmentsRepository>.value(
          value: MockAttachmentsRepository(),
        ),
      ],
      child: MultiBlocProvider(
        providers: [
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<ChatListCubit>.value(value: chatListCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: SDTFScope(
          child: Builder(
            builder: (context) {
              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
                localizationsDelegates: AppLocalizations.localizationsDelegates,
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

    testWidgets('renders muted chat correctly', (tester) async {
      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());

      final testChats = [chats[0], chats[5], chats[2]];
      when(
        () => chatListCubit.state,
      ).thenReturn(ChatListState(chatIds: testChats.map((c) => c.id).toList()));

      await tester.pumpWidget(buildSubject(chats: testChats));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_list_content_muted.png'),
      );
    });

    // The draft chat, whose preview is the draft whatever navigation does.
    final draftChat = chats[3];

    Future<void> pumpDraftChat(
      WidgetTester tester, {
      required HomeNavigationState home,
    }) async {
      when(
        () => navigationCubit.state,
      ).thenReturn(NavigationState.home(home: home));
      when(
        () => chatListCubit.state,
      ).thenReturn(ChatListState(chatIds: [draftChat.id]));

      await tester.pumpWidget(buildSubject(chats: [draftChat]));
    }

    testWidgets('shows the draft of a closed chat', (tester) async {
      await pumpDraftChat(
        tester,
        home: HomeNavigationState(chatOpen: false, chatId: draftChat.id),
      );

      expect(find.textContaining('Some draft message'), findsOne);
    });

    testWidgets('keeps the draft of the open chat', (tester) async {
      await pumpDraftChat(
        tester,
        home: HomeNavigationState(chatOpen: true, chatId: draftChat.id),
      );

      expect(find.textContaining('Some draft message'), findsOne);
    });

    Future<void> pumpReactionChat(
      WidgetTester tester, {
      required UiLastReaction reaction,
      UiMessageDraft? draft,
    }) async {
      final chat = reactedChat(reaction: reaction, draft: draft);
      when(
        () => chatListCubit.state,
      ).thenReturn(ChatListState(chatIds: [chat.id]));

      sizeView(tester, const Size(400, 120));
      await tester.pumpWidget(buildSubject(chats: [chat]));
    }

    testWidgets('reports a reaction on the last message', (tester) async {
      await pumpReactionChat(
        tester,
        reaction: UiLastReaction(reactor: 2.userId(), emoji: '👍'),
      );

      expect(find.text('Bob reacted 👍 to "Hello Alice"'), findsOne);
    });

    testWidgets('names the reader as the reactor', (tester) async {
      await pumpReactionChat(
        tester,
        reaction: UiLastReaction(reactor: 1.userId(), emoji: '👍'),
      );

      expect(find.text('You reacted 👍 to "Hello Alice"'), findsOne);
    });

    testWidgets('keeps the draft over a reaction', (tester) async {
      await pumpReactionChat(
        tester,
        reaction: UiLastReaction(reactor: 2.userId(), emoji: '👍'),
        draft: UiMessageDraft(
          message: 'Some draft message',
          editingId: null,
          updatedAt: DateTime.now(),
          isCommitted: true,
        ),
      );

      expect(find.textContaining('Some draft message'), findsOne);
      expect(find.textContaining('reacted'), findsNothing);
    });

    Future<void> pumpAttachmentChat(
      WidgetTester tester,
      UiAttachment attachment, {
      UiUserId? sender,
    }) async {
      final chat = attachmentChat(attachment, sender: sender);
      when(
        () => chatListCubit.state,
      ).thenReturn(ChatListState(chatIds: [chat.id]));

      sizeView(tester, const Size(400, 120));
      await tester.pumpWidget(buildSubject(chats: [chat]));
    }

    testWidgets('stands in for a picture with an emoji', (tester) async {
      await pumpAttachmentChat(tester, pictureAttachment);

      expect(find.text('You: 🖼️'), findsOne);
    });

    testWidgets('stands in for a file with an emoji', (tester) async {
      await pumpAttachmentChat(tester, fileAttachment);

      expect(find.text('You: 📎'), findsOne);
    });

    // A chat with a single contact names no sender, so the emoji stands alone.
    testWidgets('stands in for a picture from the contact', (tester) async {
      await pumpAttachmentChat(
        tester,
        pictureAttachment,
        sender: userProfiles[1].userId,
      );

      expect(find.text('🖼️'), findsOne);
    });

    testWidgets('stands in for a file from the contact', (tester) async {
      await pumpAttachmentChat(
        tester,
        fileAttachment,
        sender: userProfiles[1].userId,
      );

      expect(find.text('📎'), findsOne);
    });
  });
}

final pictureAttachment = UiAttachment(
  attachmentId: 1.attachmentId(),
  filename: 'image.png',
  contentType: 'image/png',
  size: 1024,
  imageMetadata: const UiImageMetadata(
    blurhash: 'LEHLk~WB2yk8pyo0adR*.7kCMdnj',
    width: 100,
    height: 50,
  ),
);

final fileAttachment = UiAttachment(
  attachmentId: 2.attachmentId(),
  filename: 'notes.pdf',
  contentType: 'application/pdf',
  size: 1024,
  imageMetadata: null,
);

/// A contact chat whose last message is [attachment], sent by [sender] (the
/// reader by default) with nothing written alongside it.
UiChatDetails attachmentChat(UiAttachment attachment, {UiUserId? sender}) =>
    UiChatDetails(
      id: 8.chatId(),
      status: const UiChatStatus.active(),
      isApq: false,
      chatType: UiChatType_Connection(userProfiles[1]),
      unreadMessages: 0,
      messagesCount: 10,
      lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
      lastMessage: UiChatMessage(
        id: 8.messageId(),
        chatId: 8.chatId(),
        timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
        message: UiMessage_Content(
          UiContentMessage(
            sender: sender ?? 1.userId(),
            sent: true,
            edited: false,
            content: UiMimiContent(
              topicId: Uint8List(0),
              attachments: [attachment],
              firstAttachmentType: attachment.imageMetadata != null
                  ? UiAttachmentType.image
                  : UiAttachmentType.file,
            ),
          ),
        ),
        status: UiMessageStatus.sent,
        reactions: [],
      ),
      mutedUntil: null,
      pendingCommitFailed: false,
    );

/// A contact chat whose last message, "Hello Alice", carries [reaction].
UiChatDetails reactedChat({
  required UiLastReaction reaction,
  UiMessageDraft? draft,
}) => UiChatDetails(
  id: 7.chatId(),
  status: const UiChatStatus.active(),
  isApq: false,
  chatType: UiChatType_Connection(userProfiles[1]),
  unreadMessages: 0,
  messagesCount: 10,
  lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
  lastMessage: UiChatMessage(
    id: 7.messageId(),
    chatId: 7.chatId(),
    timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
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
    status: UiMessageStatus.sent,
    reactions: [
      UiReaction(emoji: reaction.emoji, users: [reaction.reactor]),
    ],
  ),
  lastReaction: reaction,
  draft: draft,
  mutedUntil: null,
  pendingCommitFailed: false,
);
