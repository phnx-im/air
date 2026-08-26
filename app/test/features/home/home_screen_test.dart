// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/features/chat/share_target_publisher.dart';
import 'package:air/features/chat/chats_repository.dart' as chats_repository;
import 'package:air/features/chat_list/chat_list_view.dart';
import 'package:air/core/core.dart';
import 'package:air/features/home/home_screen.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/navigation/app_sidebar.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/you/you_pane.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/share/share_cubit.dart';
import 'package:system_date_time_format/system_date_time_format.dart';

import '../chat/chat_screen_view_test.dart';
import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../message_list/message_list_test.dart';
import '../../mocks.dart';

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
  });

  group('HomeScreen', () {
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit usersCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockUserSettingsCubit userSettingsCubit;
    late AndroidShareCubit androidShareCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      userSettingsCubit = MockUserSettingsCubit();
      androidShareCubit = MockAndroidShareCubit();

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: "untilMessageId"),
          untilTimestamp: any(named: "untilTimestamp"),
        ),
      ).thenAnswer((_) => Future.value());
      when(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: any(named: "isCommitted"),
        ),
      ).thenAnswer((_) async => Future.value());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    });

    Widget buildSubject({
      required List<UiChatDetails> chats,
    }) => MultiRepositoryProvider(
      providers: [
        RepositoryProvider<AttachmentsRepository>.value(
          value: MockAttachmentsRepository(),
        ),
        RepositoryProvider<chats_repository.ChatsRepository>.value(
          value: FakeChatsRepository(chats),
        ),
      ],
      child: MultiBlocProvider(
        providers: [
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<MessageListCubit>.value(value: messageListCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
          BlocProvider<AndroidShareCubit>.value(value: androidShareCubit),
          RepositoryProvider<ShareTargetPublisher>.value(
            value: MockShareTargetPublisher(),
          ),
        ],
        child: SDTFScope(
          child: Builder(
            builder: (context) {
              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: const HomeScreenDesktopLayout(
                  chatList: ChatListView(),
                  chat: ChatScreenView(
                    createMessageCubit: createMockMessageCubit,
                  ),
                ),
              );
            },
          ),
        ),
      ),
    );

    testWidgets('desktop layout empty', (tester) async {
      final binding = TestWidgetsFlutterBinding.ensureInitialized();
      binding.platformDispatcher.views.first.physicalSize = const Size(
        3840,
        2160,
      );
      addTearDown(() {
        binding.platformDispatcher.views.first.resetPhysicalSize();
      });

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chats[2], members: members));
      messageListCubit.setState(const []);

      await tester.pumpWidget(buildSubject(chats: []));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/home_screen_desktop_empty.png'),
      );
    }, variant: desktopPlatform);

    testWidgets('desktop layout no chat', (tester) async {
      final binding = TestWidgetsFlutterBinding.ensureInitialized();
      binding.platformDispatcher.views.first.physicalSize = const Size(
        3840,
        2160,
      );
      addTearDown(() {
        binding.platformDispatcher.views.first.resetPhysicalSize();
      });

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chats[2], members: members));
      messageListCubit.setState(messages);

      await tester.pumpWidget(buildSubject(chats: chats));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/home_screen_desktop_no_chat.png'),
      );
    }, variant: desktopPlatform);

    testWidgets('desktop layout selected chat', (tester) async {
      final binding = TestWidgetsFlutterBinding.ensureInitialized();
      binding.platformDispatcher.views.first.physicalSize = const Size(
        3840,
        2160,
      );
      addTearDown(() {
        binding.platformDispatcher.views.first.resetPhysicalSize();
      });

      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(
          home: HomeNavigationState(chatOpen: true, chatId: chats[2].id),
        ),
      );
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chats[2], members: members));
      messageListCubit.setState(messages);

      await tester.pumpWidget(buildSubject(chats: chats));
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/home_screen_desktop.png'),
      );
    }, variant: desktopPlatform);

    testWidgets('desktop layout selected blocked contact', (tester) async {
      final binding = TestWidgetsFlutterBinding.ensureInitialized();
      binding.platformDispatcher.views.first.physicalSize = const Size(
        3840,
        2160,
      );
      addTearDown(() {
        binding.platformDispatcher.views.first.resetPhysicalSize();
      });

      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(
          home: HomeNavigationState(chatOpen: true, chatId: chats[4].id),
        ),
      );
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chats[4], members: members));
      messageListCubit.setState(messages);

      await tester.pumpWidget(buildSubject(chats: chats));
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/home_screen_desktop_blocked.png'),
      );
    }, variant: desktopPlatform);

    testWidgets('desktop layout profile tab', (tester) async {
      final binding = TestWidgetsFlutterBinding.ensureInitialized();
      binding.platformDispatcher.views.first.physicalSize = const Size(
        3840,
        2160,
      );
      addTearDown(() {
        binding.platformDispatcher.views.first.resetPhysicalSize();
      });

      when(() => navigationCubit.state).thenReturn(
        const NavigationState.home(
          home: HomeNavigationState(activeTab: HomeTab.profile),
        ),
      );

      await tester.pumpWidget(buildSubject(chats: chats));
      await tester.pump();

      // The tab takes over both panes: the sections in the list panel and the
      // open one beside it. The rail stays.
      expect(find.byType(YouMenuPane), findsOneWidget);
      expect(find.byType(YouDetailPane), findsOneWidget);
      expect(find.byType(AppSidebar), findsOneWidget);
      expect(find.byType(ChatListView), findsNothing);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/home_screen_desktop_you.png'),
      );
    }, variant: desktopPlatform);
  });
}
