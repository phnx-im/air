// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/chat_details/group_members_pane.dart';
import 'package:air/features/chat_details/member_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../../mocks.dart';

final chat = chats[2];

final members = [1.userId(), 2.userId(), 3.userId()];

final profiles = [
  UiUserProfile(userId: 1.userId(), displayName: 'Alice'),
  UiUserProfile(userId: 2.userId(), displayName: 'Bob'),
  UiUserProfile(userId: 3.userId(), displayName: 'Eve'),
];

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
  });

  group('GroupMemberView', () {
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMemberDetailsCubit memberDetailsCubit;
    late MockUsersCubit usersCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      memberDetailsCubit = MockMemberDetailsCubit();
      usersCubit = MockUsersCubit();

      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chat, members: members));
      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(chatId: chat.id)),
      );
      when(
        () => memberDetailsCubit.state,
      ).thenReturn(const MemberDetailsState());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
        BlocProvider<UserCubit>.value(value: userCubit),
        BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
        BlocProvider<MemberDetailsCubit>.value(value: memberDetailsCubit),
        BlocProvider<UsersCubit>.value(value: usersCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: Scaffold(
              body: ModalSurface(child: GroupMembersPane(chatId: chat.id)),
            ),
          );
        },
      ),
    );

    testWidgets('renders correctly', (tester) async {
      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(chatId: chat.id)),
      );

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_members_screen.png'),
      );
    });

    // The list only scrolls where the modal hands it a bounded height. Neither
    // breakpoint may leave it unbounded (an overflow) nor collapse the card to
    // its minimum (a list of a few pixels).
    testWidgets('fills the full-screen modal', (tester) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(buildSubject());

      expect(tester.takeException(), isNull);
      expectFillsModal(tester, find.byType(ListView), phoneViewSize);
      expect(
        tester.getCenter(find.text('Group members')).dx,
        moreOrLessEquals(phoneViewSize.width / 2),
      );
    });

    testWidgets('fills the card modal', (tester) async {
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(buildSubject());

      expect(tester.takeException(), isNull);
      expectFillsModal(tester, find.byType(ListView), desktopViewSize);
      expect(
        tester.getCenter(find.text('Group members')).dx,
        moreOrLessEquals(desktopViewSize.width / 2),
      );
    });
  });
}
