// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/chat/create_group_screen.dart';
import 'package:air/chat/widgets/member_selection_list.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/user/user.dart';

import '../helpers.dart';
import '../mocks.dart';

const _allFeatures = AirFeatures(
  encryptedGroupProfiles: true,
  emptyConnectionGroupAttributes: true,
  pqGroups: true,
);

const _noPqFeatures = AirFeatures(
  encryptedGroupProfiles: true,
  emptyConnectionGroupAttributes: true,
  pqGroups: false,
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

  group('CreateGroupScreen', () {
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit usersCubit;

    setUp(() {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.intro());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 0));
      when(() => userCubit.contacts).thenAnswer((_) async => _contacts);
      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: _profiles));
    });

    Widget buildSubject() => MultiBlocProvider(
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
          home: const CreateGroupScreen(),
        ),
      ),
    );

    testWidgets('member selection step', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/create_group_member_selection.png'),
      );
    });

    testWidgets('details step with hidden APQ toggle revealed', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      // Select all three contacts.
      await tester.tap(find.text('Alice'));
      await tester.tap(find.text('Bob'));
      await tester.tap(find.text('Eve'));
      await tester.pumpAndSettle();

      // Advance to the details step.
      await tester.tap(find.text('Next'));
      await tester.pumpAndSettle();

      // Long-press the title to reveal the hidden APQ toggle.
      await tester.longPress(find.text('Group details'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/create_group_details_hidden_settings.png'),
      );
    });

    testWidgets(
      'details step greys out unsupported chips when APQ is enabled',
      (tester) async {
        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        // Select all three contacts.
        await tester.tap(find.text('Alice'));
        await tester.tap(find.text('Bob'));
        await tester.tap(find.text('Eve'));
        await tester.pumpAndSettle();

        // Advance to the details step.
        await tester.tap(find.text('Next'));
        await tester.pumpAndSettle();

        // Reveal the hidden APQ toggle and turn it on.
        await tester.longPress(find.text('Group details'));
        await tester.pumpAndSettle();
        await tester.tap(find.byType(Switch));
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/create_group_details_apq_enabled.png'),
        );
      },
    );

    testWidgets(
      'enabling APQ greys out unsupported contacts on the selection step',
      (tester) async {
        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        // Advance to the details step.
        await tester.tap(find.text('Next'));
        await tester.pumpAndSettle();

        // Reveal the hidden APQ toggle and turn it on.
        await tester.longPress(find.text('Group details'));
        await tester.pumpAndSettle();
        await tester.tap(find.byType(Switch));
        await tester.pumpAndSettle();

        // Go back to the selection step via the circular back button.
        final backButton = find.byWidgetPredicate(
          (w) => w is AppIcon && w.type == AppIconType.arrowLeft,
        );
        await tester.tap(backButton);
        await tester.pumpAndSettle();

        // Sanity check: we are looking at the selection step.
        expect(find.byType(MemberSelectionList), findsOneWidget);

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/create_group_selection_apq.png'),
        );
      },
    );
  });
}
