// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/group_details_view.dart';
import 'package:air/features/chat_details/mute_button.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../mocks.dart';
import '../../helpers.dart';

const desktopPhysicalSize = Size(1400, 1000);

/// Enough members to overflow the preview, in an order that only matches the
/// rendered one once sorted by display name.
final overflowingProfiles = [
  ...userProfiles,
  UiUserProfile(userId: 5.userId(), displayName: 'Dave'),
  UiUserProfile(userId: 6.userId(), displayName: 'Frank'),
  UiUserProfile(userId: 7.userId(), displayName: 'Grace'),
  UiUserProfile(userId: 8.userId(), displayName: 'Heidi'),
  UiUserProfile(userId: 9.userId(), displayName: 'Ivan'),
  UiUserProfile(userId: 10.userId(), displayName: 'Judy'),
  UiUserProfile(userId: 11.userId(), displayName: 'Mallory'),
  UiUserProfile(userId: 12.userId(), displayName: 'Niaj'),
];

void main() {
  group('GroupDetailsScreen', () {
    late MockChatDetailsCubit chatDetailsCubit;
    late MockUsersCubit usersCubit;
    late MockUserCubit userCubit;
    late MockNavigationCubit navigationCubit;

    setUp(() async {
      chatDetailsCubit = MockChatDetailsCubit();
      usersCubit = MockUsersCubit();
      userCubit = MockUserCubit();
      navigationCubit = MockNavigationCubit();

      when(
        () => userCubit.state,
      ).thenReturn(MockUiUser(id: 1, usernames: const []));
    });

    Widget buildSubject({
      List<UiUserId> members = const [],
      List<UiUserProfile> profiles = const [],
    }) {
      when(() => usersCubit.state).thenReturn(
        MockUsersState(profiles: profiles.isEmpty ? userProfiles : profiles),
      );
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chats[2], members: members));

      return MultiBlocProvider(
        providers: [
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
        ],
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testLightTheme,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const ModalScaffold(
            title: 'Group details',
            child: GroupDetailsView(),
          ),
        ),
      );
    }

    Widget buildOverflowing() => buildSubject(
      members: overflowingProfiles.map((profile) => profile.userId).toList(),
      profiles: overflowingProfiles,
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details.png'),
      );
    });

    testWidgets('renders correctly with members overflowing', (tester) async {
      await tester.pumpWidget(buildOverflowing());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details_members_overflow.png'),
      );
    });

    testWidgets('previews the first 8 members alphabetically', (tester) async {
      await tester.pumpWidget(buildOverflowing());

      // Alice is the own user, so her row reads "You" but still sorts first.
      expect(find.text('You'), findsOneWidget);
      for (final name in [
        'Bob',
        'Charlie',
        'Dave',
        'Eve',
        'Frank',
        'Grace',
        'Heidi',
      ]) {
        expect(find.text(name), findsOneWidget, reason: '$name is previewed');
      }
      for (final name in ['Ivan', 'Judy', 'Mallory', 'Niaj']) {
        expect(find.text(name), findsNothing, reason: '$name is past the cap');
      }
    });

    testWidgets('tapping the member count row opens the full list', (
      tester,
    ) async {
      await tester.pumpWidget(buildOverflowing());

      // The label, so the tap lands on the row rather than the arrow button.
      await tester.tap(find.text('12 people'));
      await tester.pump();

      verify(() => navigationCubit.openGroupMembers()).called(1);
    });

    testWidgets('renders correctly empty', (tester) async {
      await tester.pumpWidget(buildSubject(members: []));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details_empty.png'),
      );
    });

    testWidgets('renders correctly with mute menu open (mobile)', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());

      await tester.tap(find.byType(MuteButton));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details_mute_menu_mobile.png'),
      );
    });

    testWidgets('renders correctly with mute menu open (desktop)', (
      tester,
    ) async {
      tester.view.physicalSize = desktopPhysicalSize;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      await tester.pumpWidget(buildSubject());

      await tester.tap(find.byType(MuteButton));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details_mute_menu_desktop.png'),
      );
    }, variant: desktopPlatform);
  });
}
