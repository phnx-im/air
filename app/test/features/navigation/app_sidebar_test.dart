// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/nav_rail/nav_rail.dart';
import 'package:air/ds/components/nav_rail/nav_rail_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/app_sidebar.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

void main() {
  group('AppSidebar', () {
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

    Widget buildSubject({Widget rail = const AppSidebar()}) =>
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
              home: Scaffold(body: Row(children: [rail])),
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
        matchesGoldenFile('goldens/app_sidebar_chats.png'),
      );
    });

    testWidgets('renders with profile tab active', (tester) async {
      useTab(HomeTab.profile);
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/app_sidebar_profile.png'),
      );
    });

    testWidgets('tapping the inactive cell switches to its tab', (
      tester,
    ) async {
      useTab(HomeTab.chats);
      when(
        () => navigationCubit.switchTab(HomeTab.profile),
      ).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());
      await tester.tap(find.text('You'));

      verify(() => navigationCubit.switchTab(HomeTab.profile)).called(1);
    });

    Finder findPill(WidgetTester tester) {
      final palette = SemanticPalette.of(tester.element(find.byType(NavRail)));
      return find.byWidgetPredicate(
        (widget) =>
            widget is DecoratedBox &&
            widget.decoration is BoxDecoration &&
            (widget.decoration as BoxDecoration).color ==
                palette.backgroundBase.quinary,
      );
    }

    // Pinned to Linux rather than left on the host's platform: the rail only
    // reserves the window controls inset on macOS, so the strides below are the
    // layout of a desktop without it.
    testWidgets(
      'the active pill sits behind the active cell',
      (tester) async {
        useTab(HomeTab.profile);
        await tester.pumpWidget(buildSubject());

        final pill = findPill(tester);

        // Second cell of two, so the pill has moved down by one stride, and the
        // cell it marks sits inside it.
        expect(pill, findsOneWidget);
        expect(
          tester.getTopLeft(pill).dy,
          NavRailTokens.paddingTop + NavRailTokens.stride,
        );
        expect(
          tester.getTopLeft(find.text('You')).dy,
          greaterThan(tester.getTopLeft(pill).dy),
        );
      },
      variant: TargetPlatformVariant.only(TargetPlatform.linux),
    );

    testWidgets(
      'macOS reserves the rail top for the traffic lights',
      (tester) async {
        useTab(HomeTab.chats);
        await tester.pumpWidget(buildSubject());

        expect(
          tester.getTopLeft(findPill(tester)).dy,
          NavRailTokens.paddingTop + NavRailTokens.windowControlsInset,
        );
      },
      variant: TargetPlatformVariant.only(TargetPlatform.macOS),
    );

    testWidgets('reserving the window controls pushes the cells down', (
      tester,
    ) async {
      useTab(HomeTab.chats);

      await tester.pumpWidget(
        buildSubject(
          rail: NavRail(
            activeIndex: 0,
            reserveWindowControls: true,
            items: [
              NavRailItem(
                label: 'Chats',
                glyph: (_, {required active}) => const SizedBox(),
              ),
            ],
          ),
        ),
      );

      expect(
        tester.getTopLeft(find.text('Chats')).dy,
        greaterThanOrEqualTo(NavRailTokens.windowControlsInset),
      );
    });
  });
}
