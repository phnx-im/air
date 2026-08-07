// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/you/you_screen.dart';
import 'package:air/features/user/users_cubit.dart';

import '../../helpers.dart';
import '../../mocks.dart';

const physicalSize = Size(1080, 2400);

void main() {
  group('YouScreen', () {
    late MockNavigationCubit navigationCubit;
    late MockUsersCubit usersCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      usersCubit = MockUsersCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(() => usersCubit.state).thenReturn(
        MockUsersState(
          profiles: [UiUserProfile(userId: 1.userId(), displayName: "ellie")],
        ),
      );
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      when(() => navigationCubit.state).thenReturn(
        const NavigationState.home(
          home: HomeNavigationState(activeTab: HomeTab.profile),
        ),
      );
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
        BlocProvider<UsersCubit>.value(value: usersCubit),
        BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const YouScreen(),
          );
        },
      ),
    );

    Future<void> pumpSubject(WidgetTester tester) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });
      await tester.pumpWidget(buildSubject());
    }

    testWidgets('renders the section list', (tester) async {
      await pumpSubject(tester);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/you_screen.png'),
      );
    });

    testWidgets('tapping a section opens it', (tester) async {
      when(
        () => navigationCubit.openYouSection(YouSection.preferences),
      ).thenAnswer((_) async {});

      await pumpSubject(tester);
      await tester.tap(find.text('Preferences'));

      verify(
        () => navigationCubit.openYouSection(YouSection.preferences),
      ).called(1);
    });

    testWidgets('the sections behind the developer flag stay hidden', (
      tester,
    ) async {
      await pumpSubject(tester);

      expect(find.text('Devices'), findsNothing);
      expect(find.text('Developer'), findsNothing);
      expect(find.text('Profile'), findsOneWidget);
    });

    testWidgets('a developer sees devices and the developer settings', (
      tester,
    ) async {
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(isDeveloper: true));
      when(
        () => navigationCubit.openDeveloperSettings(),
      ).thenAnswer((_) async {});

      await pumpSubject(tester);

      expect(find.text('Devices'), findsOneWidget);

      await tester.tap(find.text('Developer'));

      verify(() => navigationCubit.openDeveloperSettings()).called(1);
    });
  });
}
