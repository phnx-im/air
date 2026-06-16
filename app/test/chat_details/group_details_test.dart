// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details.dart';
import 'package:air/chat/group_details.dart';
import 'package:air/chat/widgets/mute_button.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../mocks.dart';
import '../helpers.dart';

const desktopPhysicalSize = Size(1400, 1000);

void main() {
  group('GroupDetailsScreen', () {
    late MockChatDetailsCubit chatDetailsCubit;
    late MockUsersCubit usersCubit;
    late MockUserCubit userCubit;

    setUp(() async {
      chatDetailsCubit = MockChatDetailsCubit();
      usersCubit = MockUsersCubit();
      userCubit = MockUserCubit();

      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => userCubit.state,
      ).thenReturn(MockUiUser(id: 1, usernames: const []));
    });

    Widget buildSubject({List<UiUserId> members = const []}) {
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chats[2], members: members));

      return MultiBlocProvider(
        providers: [
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
        ],
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testLightTheme,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const GroupDetailsScreen(),
        ),
      );
    }

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details.png'),
      );
    });

    testWidgets('renders correctly with members overflowing', (tester) async {
      final members = userProfiles.map((e) => e.userId).toList();
      await tester.pumpWidget(buildSubject(members: members));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details_members_overflow.png'),
      );
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
    });
  });
}
