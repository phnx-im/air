// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/developer/developer_settings_section.dart';
import 'package:air/features/developer/logs_modal.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/loadable_user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/you/you_menu.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

void main() {
  group('DeveloperSettingsView', () {
    late MockLoadableUserCubit loadableUserCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() {
      loadableUserCubit = MockLoadableUserCubit();
      userSettingsCubit = MockUserSettingsCubit();

      // No user loaded, so the rows that report on one stay out of the way of
      // what these tests measure.
      when(
        () => loadableUserCubit.state,
      ).thenReturn(const LoadableUser.loading());
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(isDeveloper: true));
    });

    /// [onPaneBackground] hosts the section the way the desktop pane does, with
    /// a fill of its own between the section and the surface above it.
    Widget buildSubject({bool onPaneBackground = false}) {
      const section = SingleChildScrollView(
        child: DeveloperSettingsView(
          deviceToken: null,
          isMobile: false,
          onRefreshPushToken: _noop,
        ),
      );

      return MultiBlocProvider(
        providers: [
          BlocProvider<LoadableUserCubit>.value(value: loadableUserCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: Builder(
          builder: (context) => MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: Scaffold(
              body: onPaneBackground
                  ? const ColoredBox(color: Color(0xFF1F1E1E), child: section)
                  : section,
            ),
          ),
        ),
      );
    }

    // The section is placed and scrolled by whichever host shows it, so it
    // carries no chrome of its own: a title here would repeat the one the pane
    // header and the pushed screen already draw.
    testWidgets('renders as a bare section body', (tester) async {
      await tester.pumpWidget(buildSubject());

      expect(tester.takeException(), isNull);
      expect(find.text('Developer mode'), findsOneWidget);
      expect(find.text('Logs'), findsOneWidget);
      expect(find.byType(AppBar), findsNothing);
    });

    // The tiles here are Material's, so they look up the surface their ink
    // paints on. A host that fills its own background sits between them and the
    // one the scaffold provides, and the tiles have to bring their own to keep
    // their splashes from painting under it.
    testWidgets('carries an ink surface into a host that fills its own', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject(onPaneBackground: true));

      expect(tester.takeException(), isNull);
    });
  });

  group('LogsView', () {
    Widget buildSubject() => Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: LogsView(
          appLogs: Future.value(List.filled(200, 'a log line').join('\n')),
          backgroundLogs: Future.value(''),
          reloadLogs: _noop,
          clearLogs: _noop,
        ),
      ),
    );

    // The log text only scrolls where the modal hands the tab a bounded height.
    // Neither breakpoint may leave it unbounded (an overflow) nor collapse the
    // card towards its minimum (a tab of a few pixels).
    Future<void> expectTabFillsModal(WidgetTester tester, Size viewSize) async {
      sizeView(tester, viewSize);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      expectFillsModal(tester, find.byType(TabBarView), viewSize);
      expect(find.text('App'), findsOneWidget);
    }

    testWidgets(
      'fills the full-screen modal',
      (tester) => expectTabFillsModal(tester, phoneViewSize),
    );

    testWidgets(
      'fills the card modal',
      (tester) => expectTabFillsModal(tester, desktopViewSize),
      variant: desktopPlatform,
    );
  });

  group('YouMenu', () {
    late MockNavigationCubit navigationCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() {
      navigationCubit = MockNavigationCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home(home: HomeNavigationState()));
      when(
        () => navigationCubit.openYouSection(YouSection.developer),
      ).thenAnswer((_) async {});
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
        BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
      ],
      child: Builder(
        builder: (context) => MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const Scaffold(body: SingleChildScrollView(child: YouMenu())),
        ),
      ),
    );

    // The developer row is a section like any other, so it opens beside the
    // menu rather than over it.
    testWidgets('opens the developer section from its row', (tester) async {
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(isDeveloper: true));

      await tester.pumpWidget(buildSubject());
      await tester.tap(find.text('Developer'));

      verify(
        () => navigationCubit.openYouSection(YouSection.developer),
      ).called(1);
    });

    testWidgets('lists no developer row outside developer mode', (
      tester,
    ) async {
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(isDeveloper: false));

      await tester.pumpWidget(buildSubject());

      expect(find.text('Developer'), findsNothing);
    });
  });
}

void _noop() {}
