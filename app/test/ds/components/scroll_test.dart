// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/scroll/edge_fade.dart';
import 'package:air/ds/components/scroll/faded_scroll_frame.dart';
import 'package:air/ds/components/scroll/scroll_edges.dart';
import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

ScrollMetrics _metrics({required double pixels, double max = 500}) =>
    FixedScrollMetrics(
      minScrollExtent: 0,
      maxScrollExtent: max,
      pixels: pixels,
      viewportDimension: 600,
      axisDirection: AxisDirection.down,
      devicePixelRatio: 1,
    );

Finder _fade(FadeEdge edge) => find.byWidgetPredicate(
  (widget) => widget is EdgeFade && widget.edge == edge,
);

/// The frame's own cross-fade for [edge]. Scoped to the frame so the route
/// transition above it is never mistaken for one.
Finder _revealOf(FadeEdge edge) => find.ancestor(
  of: _fade(edge),
  matching: find.descendant(
    of: find.byType(FadedScrollFrame),
    matching: find.byType(FadeTransition),
  ),
);

double _opacityOf(WidgetTester tester, FadeEdge edge) =>
    tester.widget<FadeTransition>(_revealOf(edge).first).opacity.value;

void main() {
  group('ScrollEdges', () {
    test('reports the end the content rests against', () {
      expect(ScrollEdges.of(_metrics(pixels: 0)).atTop, isTrue);
      expect(ScrollEdges.of(_metrics(pixels: 0)).atBottom, isFalse);

      expect(ScrollEdges.of(_metrics(pixels: 500)).atBottom, isTrue);
      expect(ScrollEdges.of(_metrics(pixels: 500)).atTop, isFalse);

      final middle = ScrollEdges.of(_metrics(pixels: 250));
      expect(middle.atTop, isFalse);
      expect(middle.atBottom, isFalse);
    });

    test('counts overscroll and sub-pixel jitter as resting', () {
      expect(ScrollEdges.of(_metrics(pixels: -40)).atTop, isTrue);
      expect(ScrollEdges.of(_metrics(pixels: 0.4)).atTop, isTrue);
      expect(ScrollEdges.of(_metrics(pixels: 540)).atBottom, isTrue);
      expect(ScrollEdges.of(_metrics(pixels: 499.6)).atBottom, isTrue);
    });

    test('content shorter than the viewport rests against both ends', () {
      final edges = ScrollEdges.of(_metrics(pixels: 0, max: 0));
      expect(edges.atTop, isTrue);
      expect(edges.atBottom, isTrue);
    });

    test('compares by value, so a mid-scroll update notifies nothing', () {
      var notifications = 0;
      final notifier = ValueNotifier(ScrollEdges.of(_metrics(pixels: 250)))
        ..addListener(() => notifications++);
      addTearDown(notifier.dispose);

      notifier.value = ScrollEdges.of(_metrics(pixels: 260));
      expect(notifications, 0);

      notifier.value = ScrollEdges.of(_metrics(pixels: 500));
      expect(notifications, 1);
    });
  });

  group('EdgeFade', () {
    LinearGradient gradientOf(WidgetTester tester) {
      final container = tester.widget<Container>(find.byType(Container));
      final decoration = container.decoration! as BoxDecoration;
      return decoration.gradient! as LinearGradient;
    }

    Widget buildSubject({double opacity = 1.0, double solidStop = 0.0}) =>
        MaterialApp(
          theme: testLightTheme,
          home: Scaffold(
            body: EdgeFade(
              edge: FadeEdge.top,
              height: 96,
              color: const Color(0xFF123456),
              solidStop: solidStop,
              opacity: opacity,
            ),
          ),
        );

    testWidgets('ramps from full strength to clear', (tester) async {
      await tester.pumpWidget(buildSubject());
      final gradient = gradientOf(tester);

      expect(gradient.colors.first.a, closeTo(1.0, 0.001));
      expect(gradient.colors.last.a, closeTo(0.0, 0.001));
    });

    testWidgets('scales the whole ramp by opacity', (tester) async {
      await tester.pumpWidget(buildSubject(opacity: 0.8, solidStop: 0.4));
      final gradient = gradientOf(tester);

      // The solid head holds the peak alpha rather than a flat 1.
      expect(gradient.colors[0].a, closeTo(0.8, 0.001));
      expect(gradient.colors[1].a, closeTo(0.8, 0.001));
      expect(gradient.stops![1], closeTo(0.4, 0.001));
      expect(gradient.colors.last.a, closeTo(0.0, 0.001));
      // Every step stays under the peak.
      for (final color in gradient.colors) {
        expect(color.a, lessThanOrEqualTo(0.8 + 0.001));
      }
    });
  });

  group('FadedScrollFrame', () {
    Widget buildSubject({ValueListenable<ScrollEdges>? edges}) => MaterialApp(
      theme: testLightTheme,
      home: Scaffold(
        body: FadedScrollFrame(
          backgroundColor: const Color(0xFF123456),
          header: const SizedBox(height: 56),
          contentTopPadding: 56,
          contentBottomPadding: 120,
          edges: edges,
          builder: (topPadding, bottomPadding) => const SizedBox.expand(),
        ),
      ),
    );

    testWidgets('paints both fades when no edges are reported', (tester) async {
      await tester.pumpWidget(buildSubject());

      expect(_fade(FadeEdge.top), findsOneWidget);
      expect(_fade(FadeEdge.bottom), findsOneWidget);
      expect(_revealOf(FadeEdge.top), findsNothing);
      expect(_revealOf(FadeEdge.bottom), findsNothing);
    });

    testWidgets('hides the fade at the end the content rests against', (
      tester,
    ) async {
      final edges = ValueNotifier(ScrollEdges.atRest);
      addTearDown(edges.dispose);
      await tester.pumpWidget(buildSubject(edges: edges));

      expect(_opacityOf(tester, FadeEdge.top), 0);
      expect(_opacityOf(tester, FadeEdge.bottom), 1);

      edges.value = const ScrollEdges(atTop: false, atBottom: true);
      await tester.pumpAndSettle();

      expect(_opacityOf(tester, FadeEdge.top), 1);
      expect(_opacityOf(tester, FadeEdge.bottom), 0);
    });

    testWidgets('paints both fades mid-scroll', (tester) async {
      final edges = ValueNotifier(
        const ScrollEdges(atTop: false, atBottom: false),
      );
      addTearDown(edges.dispose);
      await tester.pumpWidget(buildSubject(edges: edges));
      await tester.pumpAndSettle();

      expect(_opacityOf(tester, FadeEdge.top), 1);
      expect(_opacityOf(tester, FadeEdge.bottom), 1);
    });
  });
}
