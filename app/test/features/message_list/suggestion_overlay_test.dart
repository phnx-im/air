// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/foundations/effects.dart';
import 'package:air/features/message_list/widgets/suggestion_overlay.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const _rowHeight = 20.0;
const _maxHeight = 100.0; // Fits exactly five rows.
const _suggestionCount = 50;

void main() {
  late SuggestionOverlayController<int> controller;
  late FocusNode focusNode;

  // Mounts the overlay with [_suggestionCount] fixed-height rows.
  Future<void> pumpOverlay(
    WidgetTester tester, {
    double maxWidth = 200,
    double rowWidth = 0,
  }) async {
    final anchorLink = LayerLink();
    focusNode = FocusNode();
    controller = SuggestionOverlayController<int>(
      vsync: const TestVSync(),
      anchorLink: anchorLink,
      focusNode: focusNode,
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: CompositedTransformTarget(
            link: anchorLink,
            child: const SizedBox(width: 200, height: 40),
          ),
        ),
      ),
    );
    unawaited(
      controller.show(
        context: tester.element(find.byType(Scaffold)),
        offset: const Offset(0, _maxHeight),
        suggestions: List.generate(_suggestionCount, (index) => index),
        style: SuggestionOverlayStyle(
          backgroundColor: Colors.white,
          borderRadius: BorderRadius.zero,
          elevation: Elevation.flat,
          maxWidth: maxWidth,
          maxHeight: _maxHeight,
        ),
        itemBuilder: (context, item, isHighlighted) =>
            SizedBox(width: rowWidth, height: _rowHeight, child: Text('$item')),
        onSelected: (_) {},
      ),
    );
    await tester.pumpAndSettle();
  }

  tearDown(() {
    controller.dispose();
    focusNode.dispose();
  });

  testWidgets('page down moves the highlight by a viewport of rows', (
    tester,
  ) async {
    await pumpOverlay(tester);
    expect(controller.highlightIndex, 0);

    controller.movePage(1);
    await tester.pumpAndSettle();
    expect(controller.highlightIndex, _maxHeight ~/ _rowHeight);

    controller.movePage(-1);
    await tester.pumpAndSettle();
    expect(controller.highlightIndex, 0);
  });

  testWidgets('paging clamps to the last suggestion', (tester) async {
    await pumpOverlay(tester);
    for (var i = 0; i < _suggestionCount; i++) {
      controller.movePage(1);
      await tester.pumpAndSettle();
    }
    expect(controller.highlightIndex, _suggestionCount - 1);
  });

  testWidgets('moving the highlight scrolls it into view', (tester) async {
    await pumpOverlay(tester);
    final scrollable = find.byType(Scrollable).last;
    expect(tester.widget<Scrollable>(scrollable).controller?.offset ?? 0, 0);

    controller.movePage(1);
    await tester.pumpAndSettle();
    // The fifth row's trailing edge sits one row below the viewport.
    expect(
      tester.state<ScrollableState>(scrollable).position.pixels,
      _rowHeight,
    );
  });

  testWidgets('a clamped key press scrolls the highlight back into view', (
    tester,
  ) async {
    // Rows need a width for the drag below to hit the scroll view.
    await pumpOverlay(tester, rowWidth: 100);
    final scrollable = find.byType(Scrollable).last;

    // Scroll the highlighted first row out of view by hand.
    await tester.drag(scrollable, const Offset(0, -_rowHeight * 3));
    await tester.pumpAndSettle();
    expect(
      tester.state<ScrollableState>(scrollable).position.pixels,
      greaterThan(0),
    );

    // Already at the top, so the highlight can't move -- but it should still
    // come back into view.
    controller.moveHighlight(-1);
    await tester.pumpAndSettle();
    expect(controller.highlightIndex, 0);
    expect(tester.state<ScrollableState>(scrollable).position.pixels, 0);
  });

  testWidgets('constrains its width to the viewport margins', (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(320, 600);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPhysicalSize);

    await pumpOverlay(tester, maxWidth: 500, rowWidth: 500);

    expect(
      controller.overlaySize.width,
      320 - suggestionOverlayViewportMargin * 2,
    );
  });
}
