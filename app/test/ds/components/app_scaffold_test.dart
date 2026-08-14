// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

const _content = ValueKey('content');

/// Content of a given height, filling the width the way a section does. A
/// zero-width box would leave the scroll view too narrow to drag on.
class _Content extends StatelessWidget {
  const _Content({required this.height});

  final double height;

  @override
  Widget build(BuildContext context) =>
      SizedBox(key: _content, height: height, width: double.infinity);
}

/// Pumps [child] in a scaffold and settles the route in, so the transition's
/// `IgnorePointer` is gone by the time a test drags on it.
Future<void> _pumpHost(WidgetTester tester, Widget child) async {
  sizeView(tester, phoneViewSize);
  await tester.pumpWidget(
    MaterialApp(
      theme: testLightTheme,
      home: AppScaffold(title: 'Section', child: child),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  group('AppScaffold', () {
    testWidgets('holds the content 40 below the bar', (tester) async {
      await _pumpHost(tester, const _Content(height: 100));

      final barBottom = tester.getBottomLeft(find.byType(AppBar)).dy;
      final contentTop = tester.getTopLeft(find.byKey(_content)).dy;
      expect(contentTop - barBottom, S.s40);
    });

    testWidgets('scrolls the gap away with the content', (tester) async {
      await _pumpHost(tester, const _Content(height: 2000));

      final barBottom = tester.getBottomLeft(find.byType(AppBar)).dy;
      await tester.drag(
        find.byType(SingleChildScrollView),
        const Offset(0, -S.s120),
      );
      await tester.pumpAndSettle();

      // The gap travels under the bar with everything else, rather than
      // holding the content away from it.
      expect(tester.getTopLeft(find.byKey(_content)).dy, lessThan(barBottom));
    });

    testWidgets('centers content that asks for the viewport', (tester) async {
      await _pumpHost(
        tester,
        const Center(
          child: SizedBox(key: _content, height: S.s24),
        ),
      );

      final barBottom = tester.getBottomLeft(find.byType(AppBar)).dy;
      // Midway down the space below the bar: the gap above the content and the
      // inset below it are the same, so they cancel out.
      expect(
        tester.getCenter(find.byKey(_content)).dy,
        (barBottom + phoneViewSize.height) / 2,
      );
    });
  });
}
