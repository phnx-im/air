// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/intro_screen.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'helpers.dart';
import 'mocks.dart';

void main() {
  group('IntroScreen', () {
    late MockLoadableUserCubit loadableUserCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() {
      loadableUserCubit = MockLoadableUserCubit();
      userSettingsCubit = MockUserSettingsCubit();
      when(
        () => loadableUserCubit.state,
      ).thenReturn(const LoadableUser.unloaded());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    });

    Widget buildSubject({bool desktop = false}) => MultiBlocProvider(
      providers: [
        BlocProvider<LoadableUserCubit>.value(value: loadableUserCubit),
        BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        BlocProvider<AppLocaleCubit>(create: (_) => AppLocaleCubit()),
      ],
      child: Builder(
        builder: (context) {
          final theme = testThemeData(MediaQuery.platformBrightnessOf(context));
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: desktop
                ? theme.copyWith(platform: desktopTargetPlatform())
                : theme,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const IntroScreen(),
          );
        },
      ),
    );

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

      await tester.pumpWidget(buildSubject(desktop: true));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/intro_screen_desktop.png'),
      );
    });
  });
}
