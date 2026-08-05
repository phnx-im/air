// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/components/scroll/edge_fade.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

Widget _host({required int rows, bool scrollable = true}) => Builder(
  builder: (context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: ModalScaffold(
      title: 'Profile',
      onTrailing: () {},
      scrollable: scrollable,
      child: scrollable
          ? Column(
              children: [
                for (var i = 0; i < rows; i++)
                  SizedBox(height: 50, child: Text('row $i')),
              ],
            )
          : ListView.builder(
              itemCount: rows,
              itemExtent: 50,
              itemBuilder: (_, i) => Text('row $i'),
            ),
    ),
  ),
);

final _fade = find.byType(EdgeFade);

/// Strength the top fade is painted at, as the reveal resolves it.
double _fadeOpacity(WidgetTester tester) => tester
    .widget<FadeTransition>(
      find.ancestor(of: _fade, matching: find.byType(FadeTransition)).first,
    )
    .opacity
    .value;

void main() {
  group('ModalScaffold', () {
    testWidgets('leaves the fade clear until content goes under the header', (
      tester,
    ) async {
      await tester.pumpWidget(_host(rows: 40));
      await tester.pumpAndSettle();
      expect(_fadeOpacity(tester), 0);

      await tester.drag(
        find.byType(SingleChildScrollView),
        const Offset(0, -200),
      );
      await tester.pumpAndSettle();
      expect(_fadeOpacity(tester), 1);

      await tester.drag(
        find.byType(SingleChildScrollView),
        const Offset(0, 200),
      );
      await tester.pumpAndSettle();
      expect(_fadeOpacity(tester), 0);
    });

    testWidgets('leaves the fade clear for a body that cannot scroll', (
      tester,
    ) async {
      await tester.pumpWidget(_host(rows: 2));
      await tester.pumpAndSettle();
      expect(_fadeOpacity(tester), 0);
    });

    testWidgets('beds the fade at the top of the scroll area', (tester) async {
      await tester.pumpWidget(_host(rows: 40));
      await tester.pumpAndSettle();

      final header = tester.getRect(find.byType(DialogHeader));
      expect(tester.getRect(_fade).top, closeTo(header.bottom, 0.01));
    });

    testWidgets('marks the scroll area with a scrollbar of its own', (
      tester,
    ) async {
      await tester.pumpWidget(_host(rows: 40));
      await tester.pumpAndSettle();

      expect(find.byType(AppScrollbar), findsOneWidget);
      // The platform's own bar would double up on the one above.
      expect(find.byType(Scrollbar), findsNothing);
    });

    testWidgets('leaves the fade and the scrollbar to a self-scrolling body', (
      tester,
    ) async {
      await tester.pumpWidget(_host(rows: 40, scrollable: false));
      await tester.pumpAndSettle();

      expect(_fade, findsNothing);
      expect(find.byType(AppScrollbar), findsNothing);
    });
  });
}
