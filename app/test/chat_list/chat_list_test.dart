// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat_list/chat_list.dart';
import 'package:air/chat_list/chat_list_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../helpers.dart';
import '../mocks.dart';
import 'chat_list_content_test.dart';

void main() {
  group('ChatList', () {
    late MockNavigationCubit navigationCubit;
    late MockChatListCubit chatListCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      chatListCubit = MockChatListCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chats[1], members: [1.userId()]));
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
          BlocProvider<UsersCubit>.value(value: contactsCubit),
          BlocProvider<ChatListCubit>.value(value: chatListCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: Builder(
          builder: (context) {
            return MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: themeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: ChatListView(
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
        matchesGoldenFile('goldens/chat_list_empty.png'),
      );
    });

    testWidgets('renders correctly', (tester) async {
      final testChats = List.generate(
        20,
        (index) => chats[index % chats.length],
      );
      final testChatIds = testChats.map((chat) => chat.id).toList();

      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(
          home: HomeNavigationState(chatOpen: true, chatId: chats[1].id),
        ),
      );
      when(
        () => chatListCubit.state,
      ).thenReturn(ChatListState(chatIds: testChatIds));

      await tester.pumpWidget(buildSubject(chats: testChats));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_list.png'),
      );
    });
  });
}
