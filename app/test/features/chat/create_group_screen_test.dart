// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/features/chat_details/create_group_modal.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';

import '../../helpers.dart';
import '../../mocks.dart';

const _allFeatures = AirFeatures(
  encryptedGroupProfiles: true,
  emptyConnectionGroupAttributes: true,
  pqGroups: true,
  apqConnectionGroups: true,
);

const _noPqFeatures = AirFeatures(
  encryptedGroupProfiles: true,
  emptyConnectionGroupAttributes: true,
  pqGroups: false,
  apqConnectionGroups: false,
);

final _profiles = [
  UiUserProfile(userId: 1.userId(), displayName: 'Alice'),
  UiUserProfile(userId: 2.userId(), displayName: 'Bob'),
  UiUserProfile(userId: 3.userId(), displayName: 'Eve'),
];

final _contacts = [
  UiContact(
    userId: 1.userId(),
    chatId: 1.chatId(),
    supportedFeatures: _allFeatures,
  ),
  UiContact(
    userId: 2.userId(),
    chatId: 2.chatId(),
    supportedFeatures: _noPqFeatures,
  ),
  UiContact(
    userId: 3.userId(),
    chatId: 3.chatId(),
    supportedFeatures: _allFeatures,
  ),
];

void main() {
  setUpAll(() {
    registerFallbackValue(0.userId());
    registerFallbackValue(0.chatId());
    registerFallbackValue(AppState.foreground);
  });

  group('CreateGroupModal', () {
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit usersCubit;

    setUp(() {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();

      when(() => navigationCubit.state)
          .thenReturn(const NavigationState.intro());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 0));
      when(() => userCubit.contacts).thenAnswer((_) async => _contacts);
      when(() => usersCubit.state)
          .thenReturn(MockUsersState(profiles: _profiles));
    });

    Widget buildSubject() {
      return MultiBlocProvider(
        providers: [
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
        ],
        child: Builder(
          builder: (context) => MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const CreateGroupModal(),
          ),
        ),
      );
    }

    testWidgets('member selection step', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/create_group_member_selection.png'),
      );
    });

    testWidgets('details step', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      // Bob has no PQ support and can't be selected.
      await tester.tap(find.text('Alice'));
      await tester.tap(find.text('Eve'));
      await tester.pumpAndSettle();

      // Advance to the details step.
      await tester.tap(find.text('Next'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/create_group_details.png'),
      );
    });
  });
}
