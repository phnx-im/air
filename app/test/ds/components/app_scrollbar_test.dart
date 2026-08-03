// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/components/scroll/app_scrollbar_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Widget _host({
  required bool reverse,
  double trackTop = 0,
  double trackBottom = 0,
}) {
  return MaterialApp(
    home: Scaffold(
      body: Center(
        child: SizedBox(
          height: 400,
          width: 300,
          child: AppScrollbar(
            trackTop: trackTop,
            trackBottom: trackBottom,
            child: ScrollConfiguration(
              behavior: const MaterialScrollBehavior().copyWith(
                scrollbars: false,
              ),
              child: ListView.builder(
                reverse: reverse,
                itemCount: 40,
                itemExtent: 50,
                itemBuilder: (_, i) => Text('item $i'),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

final _thumb = find.descendant(
  of: find.byType(AnimatedOpacity),
  matching: find.byType(Container),
);

double _opacity(WidgetTester tester) => tester
    .widget<FadeTransition>(
      find.ancestor(of: _thumb, matching: find.byType(FadeTransition)).first,
    )
    .opacity
    .value;

/// Thumb rect in the scrollbar's own coordinates.
Rect _rect(WidgetTester tester) {
  final box = tester.getRect(find.byType(AppScrollbar));
  return tester.getRect(_thumb).shift(-box.topLeft);
}

void main() {
  testWidgets('hidden at rest, shown on scroll, faded after the delay', (
    tester,
  ) async {
    await tester.pumpWidget(_host(reverse: false));
    await tester.pumpAndSettle();
    expect(_opacity(tester), 0);

    await tester.drag(find.byType(ListView), const Offset(0, -200));
    await tester.pump();
    expect(_opacity(tester), 1);

    await tester.pump(AppScrollbarTokens.hideDelay);
    await tester.pump(AppScrollbarTokens.hideDuration);
    await tester.pump(const Duration(milliseconds: 16));
    expect(_opacity(tester), 0);
  });

  testWidgets('runs top to bottom on a forward list', (tester) async {
    await tester.pumpWidget(_host(reverse: false));
    await tester.pumpAndSettle();
    final atRest = _rect(tester);
    expect(atRest.top, closeTo(2, 0.01));
    expect(atRest.width, 4);
    expect(atRest.height, closeTo(79.2, 0.01));
    expect(atRest.right, closeTo(298, 0.01));

    await tester.drag(find.byType(ListView), const Offset(0, -5000));
    await tester.pump();
    expect(_rect(tester).bottom, closeTo(398, 0.01));
  });

  testWidgets('starts at the bottom on a reversed list', (tester) async {
    await tester.pumpWidget(_host(reverse: true));
    await tester.pumpAndSettle();
    expect(_rect(tester).bottom, closeTo(398, 0.01));

    await tester.drag(find.byType(ListView), const Offset(0, 5000));
    await tester.pump();
    expect(_rect(tester).top, closeTo(2, 0.01));
  });

  testWidgets('track insets shorten both ends', (tester) async {
    await tester.pumpWidget(
      _host(reverse: false, trackTop: 80, trackBottom: 60),
    );
    await tester.pumpAndSettle();
    expect(_rect(tester).top, closeTo(82, 0.01));
    expect(_rect(tester).height, closeTo(51.2, 0.01));

    await tester.drag(find.byType(ListView), const Offset(0, -5000));
    await tester.pump();
    expect(_rect(tester).bottom, closeTo(338, 0.01));
  });
}
