// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/features/chat_list/chat_list_view.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
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
import 'chat_list_content_test.dart';

void main() {
  group('ChatList', () {
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(experimentalFeatures: false));
    });

    Widget buildSubject({
      required List<UiChatDetails> chats,
      Map<ChatId, List<UiUserId>> members = const {},
      bool shareMode = false,
    }) => RepositoryProvider<ChatsRepository>.value(
      value: FakeChatsRepository(chats, members: members),
      child: MultiBlocProvider(
        providers: [
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: contactsCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: SDTFScope(
          child: Builder(
            builder: (context) {
              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: shareMode
                    ? const ChatListView(scaffold: true, shareMode: true)
                    : const Scaffold(body: ChatListView()),
              );
            },
          ),
        ),
      ),
    );

    // The picker offers only chats the reader can send into: the connection
    // request and the blocked contact fall out, and a group the reader is not
    // a member of stays listed but disabled. Rows show group members instead
    // of the last message, and no timestamp.
    testWidgets('renders the share destination picker', (tester) async {
      final contact = chats[0];
      final request = chats[1];
      final group = chats[2];
      final otherGroup = chats[3];
      final blocked = chats[4];
      final muted = chats[5];

      await tester.pumpWidget(
        buildSubject(
          chats: [contact, request, group, otherGroup, blocked, muted],
          members: {
            group.id: [1.userId(), 2.userId(), 4.userId()],
            otherGroup.id: [2.userId(), 3.userId()],
          },
          shareMode: true,
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Bob, Charlie'), findsOne);
      expect(find.text('Bob, Eve'), findsOne);
      expect(find.text('eve_03'), findsNothing);
      expect(find.text('Charlie'), findsNothing);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_list_share_destination.png'),
      );
    });

    testWidgets('renders correctly when there are no chats', (tester) async {
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

      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(
          home: HomeNavigationState(chatOpen: true, chatId: chats[1].id),
        ),
      );

      await tester.pumpWidget(buildSubject(chats: testChats));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_list.png'),
      );
    });

    testWidgets('renders correctly with mute menu open (mobile)', (
      tester,
    ) async {
      final testChats = [chats[0]];
      await tester.pumpWidget(buildSubject(chats: testChats));

      await tester.longPress(find.text('Hello Alice'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mute'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_list_mute_menu_mobile.png'),
      );
    });

    testWidgets('renders correctly with mute menu open (desktop)', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(1400, 1000);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final testChats = [chats[0]];
      await tester.pumpWidget(buildSubject(chats: testChats));

      await tester.longPress(find.text('Hello Alice'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mute'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_list_mute_menu_desktop.png'),
      );
    }, variant: desktopPlatform);
  });
}
