// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/onboarding/intro_screen.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/user/user_session_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

void main() {
  group('IntroScreen', () {
    late MockUserSessionCubit userSessionCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() {
      userSessionCubit = MockUserSessionCubit();
      userSettingsCubit = MockUserSettingsCubit();
      when(
        () => userSessionCubit.state,
      ).thenReturn(const UserSessionState(loggedOut: true));
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<UserSessionCubit>.value(value: userSessionCubit),
        BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        BlocProvider<AppLocaleCubit>(create: (_) => AppLocaleCubit()),
      ],
      child: Builder(
        builder: (context) => MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const IntroScreen(),
        ),
      ),
    );

    testWidgets('offers onboarding once there is no user to open', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(find.text('Create account'), findsOneWidget);
    });

    testWidgets('holds back onboarding while the user loads', (tester) async {
      when(() => userSessionCubit.state).thenReturn(const UserSessionState());

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(find.text('Create account'), findsNothing);
    });

    testWidgets('holds back onboarding until a loaded user takes over', (
      tester,
    ) async {
      when(
        () => userSessionCubit.state,
      ).thenReturn(UserSessionState(user: MockUser()));

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(find.text('Create account'), findsNothing);
    });

    testWidgets('renders correctly on phone', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/intro_screen.png'),
      );
    });

    testWidgets('renders correctly on desktop', (tester) async {
      final binding = TestWidgetsFlutterBinding.ensureInitialized();
      binding.platformDispatcher.views.first.physicalSize = const Size(
        3840,
        2160,
      );
      addTearDown(() {
        binding.platformDispatcher.views.first.resetPhysicalSize();
      });

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/intro_screen_desktop.png'),
      );
    }, variant: desktopPlatform);
  });
}
