// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart' hide LogEntry;
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/components/toggle/toggle.dart';
import 'package:air/ds/components/toggle/toggle_tokens.dart';
import 'package:air/ds/foundations/foundations.dart'
    show AppIcon, AppIconType, Chrome;
import 'package:air/features/developer/developer_fields.dart';
import 'package:air/features/developer/developer_settings_section.dart';
import 'package:air/features/developer/log_entry.dart';
import 'package:air/features/developer/logs_screen.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_session_cubit.dart';
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
    late MockUserSessionCubit userSessionCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() {
      userSessionCubit = MockUserSessionCubit();
      userSettingsCubit = MockUserSettingsCubit();

      // No user loaded, so the rows reporting on one stay out of the way.
      when(() => userSessionCubit.state).thenReturn(const UserSessionState());
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(developerMode: true));
      when(
        () => userSettingsCubit.setExperimentalFeatures(
          value: any(named: 'value'),
        ),
      ).thenAnswer((_) async {});
    });

    /// [onPaneBackground] hosts the section like the desktop pane does, with
    /// its own fill behind it.
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
          BlocProvider<UserSessionCubit>.value(value: userSessionCubit),
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

    // The host places and scrolls the section, so it carries no chrome: a
    // title here would repeat the pane header's.
    testWidgets('renders as a bare section body', (tester) async {
      await tester.pumpWidget(buildSubject());

      expect(tester.takeException(), isNull);
      expect(find.text('Developer mode'), findsOneWidget);
      expect(find.text('Logs'), findsOneWidget);
      expect(find.byType(AppBar), findsNothing);
    });

    // Only the rows reporting on a user wait for one.
    testWidgets('leaves out the rows that need a user', (tester) async {
      await tester.pumpWidget(buildSubject(onPaneBackground: true));

      expect(tester.takeException(), isNull);
      expect(find.text('Diagnostics'.toUpperCase()), findsOneWidget);
      expect(find.text('Log out'), findsNothing);
      expect(find.text('Erase this database'), findsNothing);
      expect(find.text('Erase all databases'), findsOneWidget);
    });

    // The switch sits inside developer mode, so it only means anything while
    // that surface is unlocked.
    testWidgets('flips experimental features under developer mode', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());
      await tester.tap(find.text('Experimental features'));

      verify(
        () => userSettingsCubit.setExperimentalFeatures(value: true),
      ).called(1);
    });

    testWidgets('leaves experimental features inert without developer mode', (
      tester,
    ) async {
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());

      await tester.pumpWidget(buildSubject());
      await tester.tap(find.text('Experimental features'));

      verifyNever(
        () => userSettingsCubit.setExperimentalFeatures(
          value: any(named: 'value'),
        ),
      );
    });

    // An unsized AppIcon draws the asset a third larger than the other glyphs
    // on these cards, which had the rows led by their icons.
    testWidgets('draws its row glyphs at the size the surface sets', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());

      final icon = find.descendant(
        of: find.widgetWithText(ListRow, 'Logs'),
        matching: find.byType(AppIcon),
      );

      expect(
        tester.getSize(icon),
        const Size(developerRowIconSize, developerRowIconSize),
      );
    });

    // These controls are read past rather than aimed at, so they take the
    // compact density.
    testWidgets('switches at the compact density', (tester) async {
      await tester.pumpWidget(buildSubject());

      expect(
        tester.getSize(find.byType(Toggle).first),
        Size(ToggleTokens.compact.trackWidth, ToggleTokens.compact.trackHeight),
      );
    });
  });

  group('DeveloperInfoRow', () {
    Widget buildSubject() => Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        home: Scaffold(
          body: DeveloperCard(
            children: [
              ListRow(
                tokens: ListRowTokens.current,
                label: 'Logs',
                separator: false,
                trailing: const AppIcon.fileText(size: developerRowIconSize),
              ),
              const DeveloperInfoRow(label: 'User ID', value: 'user-id'),
              const DeveloperInfoRow(
                label: 'Push token',
                value: 'push-token',
                trailing: DeveloperRowButton(
                  icon: AppIconType.refreshCw,
                  tooltip: 'Refresh',
                  onPressed: _noop,
                ),
              ),
            ],
          ),
        ),
      ),
    );

    // The action used to take its width off the value's share, indenting the
    // one row that carries an action.
    testWidgets('starts its value where a row without an action does', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());

      expect(
        tester.getTopLeft(find.text('push-token')).dx,
        tester.getTopLeft(find.text('user-id')).dx,
      );
    });

    // The tap target is larger than the glyph column, so the button overhangs
    // it.
    testWidgets('ends its glyph where the plain row icons do', (tester) async {
      await tester.pumpWidget(buildSubject());

      final rowIcon = find.descendant(
        of: find.byType(ListRow),
        matching: find.byType(AppIcon),
      );
      final buttonGlyph = find.descendant(
        of: find.byType(DeveloperRowButton),
        matching: find.byType(AppIcon),
      );

      expect(tester.getRect(buttonGlyph).right, tester.getRect(rowIcon).right);
      expect(tester.getSize(buttonGlyph), tester.getSize(rowIcon));
    });
  });

  group('LogsScreenView', () {
    LogEntry entry(
      int index, {
      LogLevel level = LogLevel.info,
      String target = 'aircoreclient::qs',
    }) => LogEntry(
      time: DateTime.utc(2026, 8, 8, 14, 22, index % 60),
      level: level,
      source: LogSource.rust,
      target: target,
      message: 'record $index',
    );

    Widget buildSubject({
      required List<LogEntry> entries,
      LogBuffer buffer = LogBuffer.app,
      ValueChanged<LogBuffer>? onBufferChanged,
      LogFilter filter = const LogFilter(),
      ValueChanged<LogFilter>? onFilterChanged,
      bool following = false,
      ValueChanged<bool>? onFollowingChanged,
    }) => Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: LogsScreenView(
          buffer: buffer,
          onBufferChanged: onBufferChanged ?? (_) {},
          filter: filter,
          onFilterChanged: onFilterChanged ?? (_) {},
          following: following,
          onFollowingChanged: onFollowingChanged ?? (_) {},
          entries: entries,
          loading: false,
          error: null,
          onReload: _noop,
          onClear: _noop,
        ),
      ),
    );

    testWidgets('renders the records it is given', (tester) async {
      await tester.pumpWidget(buildSubject(entries: [entry(0), entry(1)]));

      expect(tester.takeException(), isNull);
      expect(find.text('record 0'), findsOneWidget);
      expect(find.text('record 1'), findsOneWidget);
    });

    // A full ring buffer is tens of thousands of records, only the ones on
    // screen may cost anything.
    testWidgets('builds only the rows the viewport holds', (tester) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(
        buildSubject(entries: [for (var i = 0; i < 5000; i++) entry(i)]),
      );

      expect(tester.takeException(), isNull);
      expect(find.byType(LogRow), findsWidgets);
      expect(tester.widgetList(find.byType(LogRow)).length, lessThan(100));
    });

    /// Local time, since the day header names the local day: a UTC stamp would
    /// head a different day in some zones.
    LogEntry localEntry(int index, {int day = 9}) => LogEntry(
      time: DateTime(2026, 8, day, 14, 22, index % 60),
      level: LogLevel.info,
      source: LogSource.rust,
      target: 'aircoreclient::qs',
      message: 'record $index',
    );

    // A row carries the clock alone, so only the header says which day it is.
    testWidgets('heads each day with its date', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          entries: [
            localEntry(0),
            localEntry(1, day: 8),
            localEntry(2, day: 8),
          ],
        ),
      );

      expect(find.text('2026-08-09'), findsOneWidget);
      expect(find.text('2026-08-08'), findsOneWidget);
    });

    // Pinned, so a buffer holding a whole day still says which day it is once
    // the top scrolls away.
    testWidgets('holds the day in view while its records scroll', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(
        buildSubject(entries: [for (var i = 0; i < 200; i++) localEntry(i)]),
      );
      await tester.drag(find.byType(LogRow).first, const Offset(0, -400));
      await tester.pump();

      expect(find.text('record 0'), findsNothing);
      expect(find.text('2026-08-09'), findsOneWidget);
    });

    // The header only stands for the records under it, so the next day takes
    // it over.
    testWidgets('hands the header over at a day boundary', (tester) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(
        buildSubject(
          entries: [
            for (var i = 0; i < 40; i++) localEntry(i),
            for (var i = 0; i < 40; i++) localEntry(i, day: 8),
          ],
        ),
      );
      for (var i = 0; i < 6; i++) {
        await tester.drag(find.byType(LogRow).first, const Offset(0, -400));
        await tester.pump();
      }

      expect(find.text('2026-08-09'), findsNothing);
      expect(find.text('2026-08-08'), findsOneWidget);
    });

    testWidgets('says so when the buffer holds nothing', (tester) async {
      await tester.pumpWidget(buildSubject(entries: const []));

      expect(find.text('No records'), findsOneWidget);
    });

    // The two buffers were tabs, which read as a navigation level they never
    // were.
    testWidgets('switches buffer from the segment', (tester) async {
      LogBuffer? selected;

      await tester.pumpWidget(
        buildSubject(
          entries: [entry(0)],
          onBufferChanged: (value) => selected = value,
        ),
      );
      await tester.tap(find.text('Background'));

      expect(selected, LogBuffer.background);
    });

    testWidgets('shows only what the filter passes', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          entries: [
            entry(0),
            entry(1, level: LogLevel.error),
          ],
          filter: const LogFilter(threshold: LogLevel.warn),
        ),
      );

      expect(find.text('record 1'), findsOneWidget);
      expect(find.text('record 0'), findsNothing);
    });

    // Filtering everything away is not an empty buffer, and saying so sends
    // whoever is debugging looking in the wrong place.
    testWidgets('tells an empty buffer from an empty match', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          entries: [entry(0)],
          filter: const LogFilter(query: 'nothing matches this'),
        ),
      );

      expect(find.text('No records match'), findsOneWidget);
      expect(find.text('No records'), findsNothing);
    });

    testWidgets('raises the threshold from its pill', (tester) async {
      LogFilter? changed;

      await tester.pumpWidget(
        buildSubject(
          entries: [entry(0)],
          onFilterChanged: (value) => changed = value,
        ),
      );
      await tester.tap(find.text('WARN'));

      expect(changed?.threshold, LogLevel.warn);
    });

    // Isolating a target taps the target itself, inside a row whose own tap
    // opens the record.
    testWidgets('isolates a target from its row', (tester) async {
      LogFilter? changed;

      await tester.pumpWidget(
        buildSubject(
          entries: [entry(0, target: 'aircoreclient::groups')],
          onFilterChanged: (value) => changed = value,
        ),
      );
      await tester.tap(find.text('aircoreclient::groups'));

      expect(changed?.target, 'aircoreclient::groups');
    });

    // Following holds the top of the list, where the newest record is.
    testWidgets('holds the newest record while following', (tester) async {
      sizeView(tester, phoneViewSize);
      final entries = [for (var i = 0; i < 200; i++) entry(i)];

      await tester.pumpWidget(buildSubject(entries: entries, following: true));
      await tester.drag(find.byType(LogRow).first, const Offset(0, -400));
      await tester.pump();
      // A record arriving re-pins the list, from the frame after it was added.
      await tester.pumpWidget(
        buildSubject(entries: [entry(200), ...entries], following: true),
      );
      await tester.pump();

      expect(find.text('record 200'), findsOneWidget);
    });

    // Yanking the reader back to the top a second after they scrolled is worse
    // than never following.
    testWidgets('stops following once the reader scrolls', (tester) async {
      sizeView(tester, phoneViewSize);
      bool? following;

      await tester.pumpWidget(
        buildSubject(
          entries: [for (var i = 0; i < 200; i++) entry(i)],
          following: true,
          onFollowingChanged: (value) => following = value,
        ),
      );
      await tester.drag(find.byType(LogRow).first, const Offset(0, -400));

      expect(following, isFalse);
    });

    testWidgets('drops the target filter from its pill', (tester) async {
      LogFilter? changed;

      await tester.pumpWidget(
        buildSubject(
          entries: [entry(0)],
          filter: const LogFilter(target: 'aircoreclient::qs'),
          onFilterChanged: (value) => changed = value,
        ),
      );
      // The pill carries the same text as the row it came from.
      await tester.tap(find.text('aircoreclient::qs').first);

      expect(changed?.target, isNull);
    });

    // A filter nobody can reach is a filter nobody uses, and a phone is where
    // the pills run out of width.
    testWidgets('keeps every pill on screen at phone width', (tester) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(buildSubject(entries: [entry(0)]));

      expect(tester.takeException(), isNull);
      for (final pill in find.byType(FilterPill).evaluate()) {
        final rect = tester.getRect(find.byWidget(pill.widget));
        expect(rect.left, greaterThanOrEqualTo(0));
        expect(rect.right, lessThanOrEqualTo(phoneViewSize.width));
      }
    });

    // A resize would reflow the run under the finger that tapped it, landing
    // the next tap on a different filter.
    testWidgets('keeps a pill the same width once selected', (tester) async {
      Size sizeOfWarn() => tester.getSize(
        find.ancestor(of: find.text('WARN'), matching: find.byType(FilterPill)),
      );

      await tester.pumpWidget(buildSubject(entries: [entry(0)]));
      final unselected = sizeOfWarn();

      await tester.pumpWidget(
        buildSubject(
          entries: [entry(0)],
          filter: const LogFilter(threshold: LogLevel.warn),
        ),
      );

      expect(sizeOfWarn(), unselected);
    });

    // The screen covers the window, so on macOS the traffic lights float over
    // the back button's corner.
    testWidgets(
      'clears the window controls with the back button',
      (tester) async {
        await tester.pumpWidget(buildSubject(entries: [entry(0)]));

        final button = find.descendant(
          of: find.byType(AppBarBackButton),
          matching: find.byType(ButtonIcon),
        );
        expect(
          tester.getTopLeft(button.first).dx,
          greaterThanOrEqualTo(Chrome.windowControlsInset),
        );
      },
      variant: TargetPlatformVariant.only(TargetPlatform.macOS),
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
    // menu.
    testWidgets('opens the developer section from its row', (tester) async {
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(developerMode: true));

      await tester.pumpWidget(buildSubject());
      await tester.tap(find.text('Developer'));

      verify(
        () => navigationCubit.openYouSection(YouSection.developer),
      ).called(1);
    });

    testWidgets('lists no developer row outside developer mode', (
      tester,
    ) async {
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());

      await tester.pumpWidget(buildSubject());

      expect(find.text('Developer'), findsNothing);
    });
  });
}

void _noop() {}
