// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/features/message_list/display_message_tile.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';
import '../chat_list/chat_list_content_test.dart';

/// The reader, in every test here. Eve is neither of the two people the
/// sentences name, so a name tapped in them is somebody else's.
final eve = 3.userId();

void main() {
  group('DisplayMessageTile', () {
    late MockUsersCubit usersCubit;
    late MockUserCubit userCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockNavigationCubit navigationCubit;

    setUpAll(() {
      registerFallbackValue(eve);
    });

    setUp(() {
      usersCubit = MockUsersCubit();
      userCubit = MockUserCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      navigationCubit = MockNavigationCubit();

      when(() => usersCubit.state)
          .thenReturn(MockUsersState(profiles: userProfiles));
      when(() => userCubit.state).thenReturn(MockUiUser(id: 3));
      when(() => chatDetailsCubit.state)
          .thenReturn(const ChatDetailsState(members: []));
    });

    Widget buildSubject(
      UiSystemMessage message, {
      FakeChatsRepository? repository,
    }) => MultiBlocProvider(
      providers: [
        RepositoryProvider<ChatsRepository>.value(
          value: repository ?? FakeChatsRepository(chats),
        ),
        BlocProvider<UsersCubit>.value(value: usersCubit),
        BlocProvider<UserCubit>.value(value: userCubit),
        BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
      ],
      child: Builder(
        builder: (context) => MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: Scaffold(
            body: DisplayMessageTile(
              UiEventMessage.system(message),
              DateTime.utc(2026, 1, 1),
            ),
          ),
        ),
      ),
    );

    /// The chat of a contact request of [chatType], open or [closed].
    UiChatDetails requestChat(UiChatType chatType, {bool closed = false}) =>
        UiChatDetails(
          id: 5.chatId(),
          status: closed
              ? const UiChatStatus.inactive(UiInactiveChat(pastMembers: []))
              : const UiChatStatus.active(),
          isApq: false,
          isSelfChat: false,
          chatType: chatType,
          unreadMessages: 0,
          lastUsed: DateTime.utc(2026, 1, 1),
          mutedUntil: null,
          pendingCommitFailed: false,
          resyncFailed: false,
        );

    void showChat(UiChatDetails chat) =>
        when(() => chatDetailsCubit.state)
            .thenReturn(ChatDetailsState(chat: chat, members: const []));

    testWidgets('opens the profile of the name that was tapped', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(UiSystemMessage.add(1.userId(), 2.userId())),
      );

      await tester.tapOnText(find.textRange.ofSubstring('Bob'));

      verify(() => navigationCubit.openMemberDetails(2.userId())).called(1);
    });

    testWidgets('opens nothing from the words between the names', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(UiSystemMessage.add(1.userId(), 2.userId())),
      );

      await tester.tapOnText(find.textRange.ofSubstring('added'));

      verifyNever(() => navigationCubit.openMemberDetails(any()));
    });

    testWidgets('opens nothing from the reader\'s own name', (tester) async {
      await tester.pumpWidget(
        buildSubject(UiSystemMessage.add(eve, 2.userId())),
      );

      await tester.tapOnText(find.textRange.ofSubstring('Eve'));

      verifyNever(() => navigationCubit.openMemberDetails(any()));
    });

    testWidgets('renders and opens the added name when the adder is unknown', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(UiSystemMessage.add(null, 2.userId())),
      );

      expect(
        find.text('Bob was added to the chat', findRichText: true),
        findsOneWidget,
      );

      await tester.tapOnText(find.textRange.ofSubstring('Bob'));

      verify(() => navigationCubit.openMemberDetails(2.userId())).called(1);
    });

    testWidgets(
      'renders and opens the removed name when the remover is unknown',
      (tester) async {
        await tester.pumpWidget(
          buildSubject(UiSystemMessage.remove(null, 2.userId())),
        );

        expect(
          find.text('Bob was removed from the chat', findRichText: true),
          findsOneWidget,
        );

        await tester.tapOnText(find.textRange.ofSubstring('Bob'));

        verify(() => navigationCubit.openMemberDetails(2.userId())).called(1);
      },
    );

    testWidgets('opens nothing from a group name', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.changeTitle(1.userId(), 'Weekend', 'Weekly sync'),
        ),
      );

      await tester.tapOnText(find.textRange.ofSubstring('Weekly sync'));

      verifyNever(() => navigationCubit.openMemberDetails(any()));
    });

    testWidgets('names the group chat a request came through', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.receivedAdditionalDirectConnectionRequest(
            sender: 1.userId(),
            groupChat: UiRequestGroupChat.chat(3.chatId()),
          ),
        ),
      );

      expect(
        find.text(
          'Alice also sent you a contact request through the group chat Group.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });

    testWidgets('names no group chat when it is not on this device', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.receivedAdditionalDirectConnectionRequest(
            sender: 1.userId(),
            groupChat: UiRequestGroupChat.chat(99.chatId()),
          ),
        ),
      );

      expect(
        find.text(
          'Alice also sent you a contact request through a mutual group chat.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });

    testWidgets('names the group chat once it syncs', (tester) async {
      final repository = FakeChatsRepository(chats);
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.receivedAdditionalDirectConnectionRequest(
            sender: 1.userId(),
            groupChat: UiRequestGroupChat.chat(99.chatId()),
          ),
          repository: repository,
        ),
      );
      await tester.pumpAndSettle();

      repository.upsert(groupChat(99.chatId(), 'Book Club'));
      await tester.pumpAndSettle();

      expect(
        find.text(
          'Alice also sent you a contact request through the group chat '
          'Book Club.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });

    testWidgets('names no group chat once it is deleted', (tester) async {
      final repository = FakeChatsRepository(chats);
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.receivedAdditionalDirectConnectionRequest(
            sender: 1.userId(),
            groupChat: UiRequestGroupChat.chat(3.chatId()),
          ),
          repository: repository,
        ),
      );
      await tester.pumpAndSettle();

      repository.remove(3.chatId());
      await tester.pumpAndSettle();

      expect(
        find.text(
          'Alice also sent you a contact request through a mutual group chat.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });

    testWidgets('renames the group chat on the request card', (tester) async {
      showChat(requestChat(UiChatType_PendingConnection(userProfiles[0])));
      final repository = FakeChatsRepository(chats);
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.receivedDirectConnectionRequest(
            sender: 1.userId(),
            groupChat: UiRequestGroupChat.chat(3.chatId()),
          ),
          repository: repository,
        ),
      );
      await tester.pumpAndSettle();

      repository.upsert(groupChat(3.chatId(), 'Book Club'));
      await tester.pumpAndSettle();

      expect(
        find.text(
          'Alice sent you a contact request through the group chat Book Club.',
        ),
        findsOneWidget,
      );
    });

    testWidgets('shows a request card without a group chat on this device', (
      tester,
    ) async {
      showChat(requestChat(UiChatType_PendingConnection(userProfiles[0])));
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.receivedDirectConnectionRequest(
            sender: 1.userId(),
            groupChat: UiRequestGroupChat.chat(99.chatId()),
          ),
        ),
      );

      expect(
        find.text(
          'Alice sent you a contact request through a mutual group chat.',
        ),
        findsOneWidget,
      );
    });

    testWidgets('names the group chat a sent request went through', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.sentDirectConnectionRequest(
            recipient: 1.userId(),
            originChatId: 3.chatId(),
          ),
        ),
      );

      expect(
        find.text(
          'You sent a contact request to Alice through the group chat Group.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });

    testWidgets('names no group chat of a sent request not on this device', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.sentDirectConnectionRequest(
            recipient: 1.userId(),
            originChatId: 99.chatId(),
          ),
        ),
      );

      expect(
        find.text(
          'You sent a contact request to Alice through a mutual group chat.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });

    testWidgets('shows an unavailable request as a record', (tester) async {
      showChat(
        requestChat(
          UiChatType_PendingConnection(userProfiles[0]),
          closed: true,
        ),
      );
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.receivedHandleConnectionRequest(
            sender: 1.userId(),
            username: const UiUsername(plaintext: 'eve_03'),
          ),
        ),
      );

      expect(find.text('Accept'), findsNothing);
      expect(
        find.text(
          'Alice sent you a contact request through your username eve_03.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });

    testWidgets('shows the request card while the request is open', (
      tester,
    ) async {
      showChat(requestChat(UiChatType_PendingConnection(userProfiles[0])));
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.receivedHandleConnectionRequest(
            sender: 1.userId(),
            username: const UiUsername(plaintext: 'eve_03'),
          ),
        ),
      );

      expect(find.text('Accept'), findsOneWidget);
    });

    testWidgets('shows the card of an open request sent to a username', (
      tester,
    ) async {
      const username = UiUsername(plaintext: 'eve_03');
      showChat(requestChat(const UiChatType_HandleConnection(username)));
      await tester.pumpWidget(
        buildSubject(const UiSystemMessage.newHandleConnectionChat(username)),
      );

      expect(find.text('You sent a contact request'), findsOneWidget);
      expect(find.text('to the username eve_03'), findsOneWidget);
      expect(
        find.text("You'll be able to chat once they accept."),
        findsOneWidget,
      );
      expect(find.text('Retract request'), findsOneWidget);
    });

    testWidgets('shows the card of an open request sent through a group', (
      tester,
    ) async {
      showChat(
        requestChat(UiChatType_TargetedMessageConnection(userProfiles[0])),
      );
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.sentDirectConnectionRequest(
            recipient: 1.userId(),
            originChatId: 3.chatId(),
          ),
        ),
      );

      expect(find.text('You sent Alice a contact request'), findsOneWidget);
      expect(find.text('via your group Group'), findsOneWidget);
      expect(find.text('Retract request'), findsOneWidget);
    });

    testWidgets('names no group of a sent request not on this device', (
      tester,
    ) async {
      showChat(
        requestChat(UiChatType_TargetedMessageConnection(userProfiles[0])),
      );
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.sentDirectConnectionRequest(
            recipient: 1.userId(),
            originChatId: 99.chatId(),
          ),
        ),
      );

      expect(find.text('You sent Alice a contact request'), findsOneWidget);
      expect(find.textContaining('via your group'), findsNothing);
    });

    testWidgets('shows a closed sent request as a record', (tester) async {
      const username = UiUsername(plaintext: 'eve_03');
      showChat(
        requestChat(const UiChatType_HandleConnection(username), closed: true),
      );
      await tester.pumpWidget(
        buildSubject(const UiSystemMessage.newHandleConnectionChat(username)),
      );

      expect(find.text('Retract request'), findsNothing);
      expect(
        find.textContaining(
          'You sent a contact request to username eve_03.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });

    testWidgets('renders an unavailable request', (tester) async {
      await tester.pumpWidget(
        buildSubject(const UiSystemMessage.connectionRequestUnavailable()),
      );
      expect(
        find.text(
          'The contact request is no longer available.',
          findRichText: true,
        ),
        findsOneWidget,
      );
    });
  });
}
