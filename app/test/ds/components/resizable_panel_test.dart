// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/resizable_panel/resizable_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import '../../helpers.dart';

void main() {
  group('ResizablePanel', () {
    const childKey = ValueKey('pane');
    const initialWidth = 300.0;

    double? reportedWidth;

    setUp(() {
      reportedWidth = null;
    });

    Widget buildSubject() => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testLightTheme,
      home: Scaffold(
        body: ResizablePanel(
          initialWidth: initialWidth,
          onResizeEnd: (width) => reportedWidth = width,
          panelBuilder: (context, width) => SizedBox(
            width: width,
            height: double.infinity,
            child: const ColoredBox(key: childKey, color: Color(0xFF00FF00)),
          ),
          content: const SizedBox.expand(),
        ),
      ),
    );

    /// Pumps on a viewport wide enough to hold the panel at its maximum width.
    Future<void> pumpSubject(WidgetTester tester) async {
      tester.view.physicalSize = const Size(1600, 1200);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(buildSubject());
    }

    double handleOpacity(WidgetTester tester) =>
        tester.widget<AnimatedOpacity>(find.byType(AnimatedOpacity)).opacity;

    Offset handleCenter(WidgetTester tester) {
      final pane = tester.getRect(find.byKey(childKey));
      return Offset(pane.right + resizeHandleInset, pane.center.dy);
    }

    testWidgets('builds the pane at the panel width', (tester) async {
      await pumpSubject(tester);

      expect(tester.getSize(find.byKey(childKey)).width, initialWidth);
    });

    testWidgets('sits the handle in the gutter beside the panel', (
      tester,
    ) async {
      await pumpSubject(tester);

      final pane = tester.getRect(find.byKey(childKey));
      final handle = tester.getRect(find.byType(AnimatedOpacity));

      expect(handle.center.dx, pane.right + resizeHandleInset);
    });

    testWidgets('reveals the handle on hover only', (tester) async {
      await pumpSubject(tester);

      expect(handleOpacity(tester), 0);

      final pointer = await tester.createGesture(kind: .mouse);
      await pointer.addPointer(location: Offset.zero);
      addTearDown(pointer.removePointer);

      await pointer.moveTo(handleCenter(tester));
      await tester.pumpAndSettle();

      expect(handleOpacity(tester), 1);

      await pointer.moveTo(Offset.zero);
      await tester.pumpAndSettle();

      expect(handleOpacity(tester), 0);
    });

    testWidgets('resizes on drag and reports the final width', (tester) async {
      await pumpSubject(tester);

      final drag = await tester.startGesture(handleCenter(tester));
      await drag.moveBy(const Offset(50, 0));
      await tester.pump();

      expect(handleOpacity(tester), 1);
      expect(tester.getSize(find.byKey(childKey)).width, initialWidth + 50);

      await drag.up();
      await tester.pumpAndSettle();

      expect(reportedWidth, initialWidth + 50);
      expect(handleOpacity(tester), 0);
    });

    testWidgets('keeps the handle lit while a drag runs past the edge', (
      tester,
    ) async {
      await pumpSubject(tester);

      final pointer = await tester.createGesture(kind: .mouse);
      await pointer.addPointer(location: Offset.zero);
      addTearDown(pointer.removePointer);

      await pointer.moveTo(handleCenter(tester));
      await pointer.down(handleCenter(tester));
      // At the maximum width the panel stops following, leaving the pointer
      // well clear of the handle's narrow target.
      await pointer.moveBy(const Offset(500, 0));
      await tester.pumpAndSettle();

      expect(handleOpacity(tester), 1);

      await pointer.up();
      await tester.pumpAndSettle();

      expect(handleOpacity(tester), 0);
    });

    testWidgets('tracks the pointer after a drag hits a bound', (tester) async {
      await pumpSubject(tester);

      final drag = await tester.startGesture(handleCenter(tester));
      await drag.moveBy(const Offset(500, 0));
      await tester.pump();

      expect(tester.getSize(find.byKey(childKey)).width, 600);

      await drag.moveBy(const Offset(-300, 0));
      await tester.pump();

      // Width follows the pointer's total offset, so overshooting the maximum
      // does not leave the handle trailing behind the cursor on the way back.
      expect(tester.getSize(find.byKey(childKey)).width, initialWidth + 200);

      await drag.up();
      await tester.pumpAndSettle();
    });
  });
}
