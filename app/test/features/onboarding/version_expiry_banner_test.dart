// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/features/onboarding/update_required_screen.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

void main() {
  group('UpdateRequiredScreen', () {
    late MockUserCubit userCubit;
    late MockUserSettingsCubit userSettingsCubit;

    Widget buildSubject({Widget child = const Text('content')}) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: MultiBlocProvider(
            providers: [
              BlocProvider<UserCubit>.value(value: userCubit),
              BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
            ],
            child: UpdateRequiredScreen(child: child),
          ),
        );
      },
    );

    final expiresAt = DateTime.utc(2026, 8, 1, 12);

    void announceExpiry() {
      when(() => userCubit.state).thenReturn(
        MockUiUser(id: 1, versionStatus: VersionStatus.expiresAt(expiresAt)),
      );
    }

    /// Stands in for the app content under the banner.
    const content = Scaffold(body: Center(child: Text('content')));

    setUp(() {
      userCubit = MockUserCubit();
      userSettingsCubit = MockUserSettingsCubit();
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      when(
        () => userSettingsCubit.setDismissedVersionExpiry(
          value: any(named: 'value'),
        ),
      ).thenAnswer((_) async {});
    });

    testWidgets('shows only the content when the version is supported', (
      tester,
    ) async {
      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));

      await tester.pumpWidget(buildSubject());

      expect(find.text('content'), findsOneWidget);
      expect(find.textContaining('Update Air by'), findsNothing);
    });

    testWidgets('shows the banner when the version expires and dismisses it', (
      tester,
    ) async {
      announceExpiry();

      await tester.pumpWidget(buildSubject());

      expect(find.text('content'), findsOneWidget);
      expect(find.textContaining('Update Air by'), findsOneWidget);

      await tester.tap(find.byType(ButtonIcon));
      await tester.pump();

      verify(
        () => userSettingsCubit.setDismissedVersionExpiry(value: expiresAt),
      ).called(1);
    });

    testWidgets('hides the banner dismissed for the announced expiry', (
      tester,
    ) async {
      announceExpiry();
      when(
        () => userSettingsCubit.state,
      ).thenReturn(UserSettings(dismissedVersionExpiry: expiresAt));

      await tester.pumpWidget(buildSubject());

      expect(find.text('content'), findsOneWidget);
      expect(find.textContaining('Update Air by'), findsNothing);
    });

    testWidgets('shows the banner dismissed for an earlier expiry', (
      tester,
    ) async {
      announceExpiry();
      when(() => userSettingsCubit.state).thenReturn(
        UserSettings(
          dismissedVersionExpiry: expiresAt.subtract(const Duration(days: 7)),
        ),
      );

      await tester.pumpWidget(buildSubject());

      expect(find.textContaining('Update Air by'), findsOneWidget);
    });

    testWidgets('replaces the content when the version is unsupported', (
      tester,
    ) async {
      when(() => userCubit.state).thenReturn(
        MockUiUser(id: 1, versionStatus: const VersionStatus.unsupported()),
      );

      await tester.pumpWidget(buildSubject());

      expect(find.text('content'), findsNothing);
      expect(find.text('Software update required'), findsOneWidget);
    });

    testWidgets('banner renders correctly', (tester) async {
      sizeView(tester, phoneViewSize);
      announceExpiry();

      await tester.pumpWidget(buildSubject(child: content));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/version_expiry_banner.png'),
      );
    });

    testWidgets('banner renders correctly (dark mode)', (tester) async {
      sizeView(tester, phoneViewSize);
      tester.platformDispatcher.platformBrightnessTestValue = .dark;
      addTearDown(() {
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });
      announceExpiry();

      await tester.pumpWidget(buildSubject(child: content));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/version_expiry_banner_dark_mode.png'),
      );
    });

    testWidgets('banner renders correctly (desktop)', (tester) async {
      sizeView(tester, desktopViewSize);
      announceExpiry();

      await tester.pumpWidget(buildSubject(child: content));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/version_expiry_banner_desktop.png'),
      );
    }, variant: desktopPlatform);
  });
}
