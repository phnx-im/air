// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/navigation/app_tab_bar.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

void main() {
  group('AppTabBar', () {
    late MockNavigationCubit navigationCubit;
    late MockUsersCubit usersCubit;

    setUp(() {
      navigationCubit = MockNavigationCubit();
      usersCubit = MockUsersCubit();

      when(() => usersCubit.state).thenReturn(
        MockUsersState(
          profiles: [UiUserProfile(userId: 1.userId(), displayName: 'Alice')],
        ),
      );
    });

    Widget buildSubject({Widget home = const _ScaffoldedTabBar()}) =>
        MultiBlocProvider(
          providers: [
            BlocProvider<NavigationCubit>.value(value: navigationCubit),
            BlocProvider<UsersCubit>.value(value: usersCubit),
          ],
          child: Builder(
            builder: (context) => MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: home,
            ),
          ),
        );

    void useTab(HomeTab tab) {
      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(activeTab: tab)),
      );
    }

    testWidgets('renders with chats tab active', (tester) async {
      useTab(HomeTab.chats);
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/app_tab_bar_chats.png'),
      );
    });

    testWidgets('renders with profile tab active', (tester) async {
      useTab(HomeTab.profile);
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/app_tab_bar_profile.png'),
      );
    });

    testWidgets('renders with chats tab active (dark mode)', (tester) async {
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);

      useTab(HomeTab.chats);
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/app_tab_bar_chats_dark.png'),
      );
    });

    testWidgets('tapping the inactive tab switches to it', (tester) async {
      useTab(HomeTab.chats);
      when(
        () => navigationCubit.switchTab(HomeTab.profile),
      ).thenAnswer((_) async {});
      await tester.pumpWidget(buildSubject());

      await tester.tap(find.text('You'));

      verify(() => navigationCubit.switchTab(HomeTab.profile)).called(1);
    });

    testWidgets('tapping the inactive chats tab switches to it', (
      tester,
    ) async {
      useTab(HomeTab.profile);
      when(
        () => navigationCubit.switchTab(HomeTab.chats),
      ).thenAnswer((_) async {});
      await tester.pumpWidget(buildSubject());

      await tester.tap(find.text('Chats'));

      verify(() => navigationCubit.switchTab(HomeTab.chats)).called(1);
    });

    // The mobile home layout stacks the bar over the tab content rather than
    // inside a Scaffold, so the bar has to bring its own Material. Without one
    // every label picks up the missing-Material marker: a yellow double
    // underline.
    testWidgets('labels carry no text decoration outside a Scaffold', (
      tester,
    ) async {
      useTab(HomeTab.chats);
      await tester.pumpWidget(
        buildSubject(
          home: const Stack(
            children: [
              Positioned(left: 0, right: 0, bottom: 0, child: AppTabBar()),
            ],
          ),
        ),
      );

      for (final label in ['Chats', 'You']) {
        final text = tester.widget<RichText>(
          find.descendant(
            of: find.text(label),
            matching: find.byType(RichText),
          ),
        );
        expect(
          text.text.style?.decoration ?? TextDecoration.none,
          TextDecoration.none,
          reason: '$label label inherited a decoration',
        );
      }
    });
  });
}

/// The bar as the golden tests frame it: centered on a plain Scaffold.
class _ScaffoldedTabBar extends StatelessWidget {
  const _ScaffoldedTabBar();

  @override
  Widget build(BuildContext context) => const Scaffold(
    backgroundColor: Color(0xFFEEEEEE),
    body: Center(child: AppTabBar()),
  );
}
