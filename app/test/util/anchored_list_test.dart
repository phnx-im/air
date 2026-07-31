// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/util/anchored_list/anchored_list.dart';
import 'package:air/util/anchored_list/controller.dart';
import 'package:air/util/anchored_list/data.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Widget buildSubject({
    required AnchoredListData<int> data,
    required AnchoredListController controller,
    required bool canLoadNewer,
    required VoidCallback onLoadNewer,
    double viewportHeight = 800,
    Map<int, double> itemHeights = const {},
    double topPadding = 0.0,
    double bottomPadding = 0.0,
  }) {
    return MaterialApp(
      home: Scaffold(
        body: Center(
          child: SizedBox(
            width: 400,
            height: viewportHeight,
            child: AnchoredList<int>(
              data: data,
              controller: controller,
              idExtractor: (item) => item,
              canLoadOlder: false,
              canLoadNewer: canLoadNewer,
              paginationThreshold: 100,
              onLoadNewer: onLoadNewer,
              topPadding: topPadding,
              bottomPadding: bottomPadding,
              itemBuilder: (context, item, index) => KeyedSubtree(
                key: ValueKey('item-$item'),
                child: SizedBox(
                  height: itemHeights[item] ?? 100,
                  child: const ColoredBox(color: Colors.blue),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  group('AnchoredList pagination', () {
    testWidgets('prefetches newer messages before the bottom edge', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final data = AnchoredListData<int>(List.generate(30, (index) => index));
      final controller = AnchoredListController();
      var loadNewerCalls = 0;

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: false,
          onLoadNewer: () {
            loadNewerCalls++;
          },
        ),
      );
      await tester.pump();

      final position = controller.position!;
      expect(position.maxScrollExtent, greaterThan(900));

      position.jumpTo(950);
      await tester.pump();

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: true,
          onLoadNewer: () {
            loadNewerCalls++;
          },
        ),
      );
      await tester.pump();

      position.jumpTo(750);
      await tester.pump();

      expect(loadNewerCalls, 1);
    });

    testWidgets('rechecks the newer edge when availability flips on', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final data = AnchoredListData<int>(List.generate(30, (index) => index));
      final controller = AnchoredListController();
      var loadNewerCalls = 0;

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: false,
          onLoadNewer: () {
            loadNewerCalls++;
          },
        ),
      );
      await tester.pump();

      final position = controller.position!;
      position.jumpTo(750);
      await tester.pump();
      expect(loadNewerCalls, 0);

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: true,
          onLoadNewer: () {
            loadNewerCalls++;
          },
        ),
      );
      await tester.pump();
      await tester.pump();

      expect(loadNewerCalls, 1);
    });

    testWidgets('tracks the newest visible item using measured heights', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 250);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final data = AnchoredListData<int>([0, 1, 2]);
      final controller = AnchoredListController();

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: false,
          onLoadNewer: () {},
          viewportHeight: 250,
          itemHeights: const {0: 100, 1: 300, 2: 100},
        ),
      );
      await tester.pump();

      expect(controller.currentNewestVisibleId, 0);

      controller.position!.jumpTo(90);
      await tester.pump();
      expect(controller.currentNewestVisibleId, 0);

      controller.position!.jumpTo(controller.position!.maxScrollExtent);
      await tester.pump();
      await tester.pump();

      expect(find.byKey(const ValueKey('item-0')), findsNothing);
      expect(controller.currentNewestVisibleId, 1);
    });

    testWidgets('excludes items hidden behind the bottom inset', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 300);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      // Viewport 300 with a 100px bottom inset (e.g. an overlaid composer).
      // The unobscured area is [0, 200]; the bottom [200, 300] is covered.
      final data = AnchoredListData<int>([0, 1, 2, 3, 4, 5]);
      final controller = AnchoredListController();

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: false,
          onLoadNewer: () {},
          viewportHeight: 300,
          bottomPadding: 100,
        ),
      );
      await tester.pump();

      // Scroll up so item 0 sits entirely behind the bottom inset and item 1
      // straddles its top edge.
      controller.position!.jumpTo(150);
      await tester.pump();

      // item 0 is laid out but fully obscured, so the newest *visible* item is
      // item 1, not item 0.
      expect(find.byKey(const ValueKey('item-0')), findsOneWidget);
      expect(controller.currentNewestVisibleId, 1);
    });
  });

  group('AnchoredList jump', () {
    Future<Rect> rectOf(WidgetTester tester, Object id) async {
      final finder = find.byKey(ValueKey('item-$id'));
      final renderBox = tester.renderObject<RenderBox>(finder);
      final topLeft = renderBox.localToGlobal(Offset.zero);
      return topLeft & renderBox.size;
    }

    testWidgets('off-screen jump lands target below the top inset', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final data = AnchoredListData<int>(List.generate(40, (index) => index));
      final controller = AnchoredListController();
      const topPadding = 120.0;

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: false,
          onLoadNewer: () {},
          topPadding: topPadding,
        ),
      );
      await tester.pump();

      controller.goToId(20, intent: JumpIntent.quotedMessage);
      await tester.pumpAndSettle();

      final viewportTopY = tester.getTopLeft(find.byType(AnchoredList<int>)).dy;
      final targetRect = await rectOf(tester, 20);
      final relativeTop = targetRect.top - viewportTopY;
      expect(relativeTop, closeTo(topPadding, 0.5));
    });

    testWidgets(
      'on-screen jump aligns target when it straddles the top inset',
      (tester) async {
        tester.view.physicalSize = const Size(400, 800);
        tester.view.devicePixelRatio = 1.0;
        addTearDown(() {
          tester.view.resetPhysicalSize();
          tester.view.resetDevicePixelRatio();
        });

        final data = AnchoredListData<int>(List.generate(40, (index) => index));
        final controller = AnchoredListController();
        const topPadding = 120.0;

        await tester.pumpWidget(
          buildSubject(
            data: data,
            controller: controller,
            canLoadNewer: false,
            onLoadNewer: () {},
            topPadding: topPadding,
          ),
        );
        await tester.pump();

        // Land at item 20 (top inset), then nudge the scroll so item 18
        // sits partially under the top inset — clipped, not fully visible.
        controller.goToId(20, intent: JumpIntent.quotedMessage);
        await tester.pumpAndSettle();
        controller.position!.jumpTo(controller.position!.pixels - 240);
        await tester.pump();

        final viewportTopY = tester
            .getTopLeft(find.byType(AnchoredList<int>))
            .dy;
        final clippedTop = (await rectOf(tester, 18)).top - viewportTopY;
        expect(clippedTop, lessThan(topPadding));
        expect(clippedTop, greaterThan(0));

        controller.goToId(18, intent: JumpIntent.quotedMessage);
        await tester.pumpAndSettle();

        final alignedTop = (await rectOf(tester, 18)).top - viewportTopY;
        expect(alignedTop, closeTo(topPadding, 0.5));
      },
    );

    testWidgets('on-screen jump leaves a fully-visible target in place', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final data = AnchoredListData<int>(List.generate(40, (index) => index));
      final controller = AnchoredListController();
      const topPadding = 120.0;

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: false,
          onLoadNewer: () {},
          topPadding: topPadding,
        ),
      );
      await tester.pump();

      // After landing at 20 (top inset), 18 sits two rows below it, well
      // inside the unobscured viewport.
      controller.goToId(20, intent: JumpIntent.quotedMessage);
      await tester.pumpAndSettle();

      final pixelsBefore = controller.position!.pixels;
      final rectBefore = await rectOf(tester, 18);

      controller.goToId(18, intent: JumpIntent.quotedMessage);
      await tester.pumpAndSettle();

      // No scroll, no movement: highlight is the only effect.
      expect(controller.position!.pixels, pixelsBefore);
      expect((await rectOf(tester, 18)).top, rectBefore.top);
    });

    testWidgets('jumps land below the top inset across clustered heights', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      // Clusters of dissimilar heights — average across the list does not
      // match any cluster's actual height. This catches alignment that
      // relies on cache averages instead of measuring the rendered box.
      final heights = <int, double>{
        for (var i = 0; i < 40; i++)
          i: i < 10
              ? 40.0
              : i < 25
              ? 200.0
              : 80.0,
      };
      final data = AnchoredListData<int>(List.generate(40, (index) => index));
      final controller = AnchoredListController();
      const topPadding = 100.0;

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: false,
          onLoadNewer: () {},
          topPadding: topPadding,
          itemHeights: heights,
        ),
      );
      await tester.pump();

      // Probe across each cluster.
      for (final target in const [12, 20, 30, 35]) {
        controller.goToId(target, intent: JumpIntent.quotedMessage);
        await tester.pumpAndSettle();

        final viewportTopY = tester
            .getTopLeft(find.byType(AnchoredList<int>))
            .dy;
        final targetRect = await rectOf(tester, target);
        final relativeTop = targetRect.top - viewportTopY;
        expect(
          relativeTop,
          closeTo(topPadding, 0.5),
          reason: 'item $target landed at $relativeTop, expected $topPadding',
        );
      }
    });

    testWidgets('jumps land below the top inset for variable item heights', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      // Mix of heights: short and tall items alternate.
      final heights = <int, double>{
        for (var i = 0; i < 40; i++) i: i.isEven ? 60.0 : 180.0,
      };
      final data = AnchoredListData<int>(List.generate(40, (index) => index));
      final controller = AnchoredListController();
      const topPadding = 120.0;

      await tester.pumpWidget(
        buildSubject(
          data: data,
          controller: controller,
          canLoadNewer: false,
          onLoadNewer: () {},
          topPadding: topPadding,
          itemHeights: heights,
        ),
      );
      await tester.pump();

      // Probe several targets — both odd (tall) and even (short).
      for (final target in const [10, 13, 20, 25, 30]) {
        controller.goToId(target, intent: JumpIntent.quotedMessage);
        await tester.pumpAndSettle();

        final viewportTopY = tester
            .getTopLeft(find.byType(AnchoredList<int>))
            .dy;
        final targetRect = await rectOf(tester, target);
        final relativeTop = targetRect.top - viewportTopY;
        expect(
          relativeTop,
          closeTo(topPadding, 0.5),
          reason: 'item $target landed at $relativeTop, expected $topPadding',
        );
      }
    });
  });

  group('AnchoredList frame safety', () {
    // Three things have to land on one frame: an insert at index 0 (which
    // shifts every live child's index), a height change applied during the
    // viewport's own layout, and a settling ballistic scroll. The settle is
    // what dispatches a ScrollEndNotification from inside layout, and
    // handling it there read a half-laid-out sliver, whose moved children
    // still have a null layoutOffset.
    //
    // Images are the common trigger because their row resolves its real
    // height a frame or more after the insert, at a time nothing
    // coordinates -- so it can coincide with a settle. A text row is at its
    // final height on the insert frame itself.
    Widget buildResizingSubject({
      required AnchoredListData<int> data,
      required AnchoredListController controller,
      required Map<int, double> heights,
      required ValueListenable<double> bottomPadding,
      required double viewportHeight,
    }) {
      return MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 400,
            height: viewportHeight,
            child: ValueListenableBuilder<double>(
              valueListenable: bottomPadding,
              builder: (context, padding, _) => AnchoredList<int>(
                data: data,
                controller: controller,
                idExtractor: (item) => item,
                canLoadOlder: false,
                canLoadNewer: false,
                topPadding: 100,
                bottomPadding: padding,
                itemBuilder: (context, item, index) => SizedBox(
                  height: heights[item] ?? 40,
                  child: const ColoredBox(color: Colors.blue),
                ),
              ),
            ),
          ),
        ),
      );
    }

    testWidgets('sending an image while the list is still settling', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final heights = <int, double>{for (var i = 0; i < 3000; i++) i: 40.0};
      for (var i = 0; i < 3000; i += 9) {
        heights[i] = 900.0;
      }
      final data = AnchoredListData<int>(List.generate(3000, (i) => i));
      final controller = AnchoredListController();
      // Insets stay constant: the row's own decode is the only dimension
      // change, so nothing here depends on the composer.
      final bottomPadding = ValueNotifier<double>(80);
      addTearDown(bottomPadding.dispose);

      await tester.pumpWidget(
        buildResizingSubject(
          data: data,
          controller: controller,
          heights: heights,
          bottomPadding: bottomPadding,
          viewportHeight: 600,
        ),
      );
      await tester.pumpAndSettle();

      // Flick upward and leave the list coasting, so the frames below land
      // while a ballistic activity is still settling.
      await tester.fling(
        find.byType(CustomScrollView),
        const Offset(0, 300),
        800,
      );
      await tester.pump(const Duration(milliseconds: 40));

      for (var n = 0; n < 12; n++) {
        final id = 3000 + n;
        data.insert(0, id);
        heights[id] = 30; // placeholder, before the image decodes
        await tester.pump(const Duration(milliseconds: 16));
        expect(tester.takeException(), isNull, reason: 'insert $n');

        heights[id] = 700; // decoded, at its real height
        await tester.pump(const Duration(milliseconds: 16));
        expect(tester.takeException(), isNull, reason: 'decode $n');
      }
    });

    testWidgets('stays valid when padding exceeds the viewport', (
      tester,
    ) async {
      // bottomPadding tracks the composer, so the keyboard opening can leave
      // the combined insets taller than the remaining viewport.
      tester.view.physicalSize = const Size(400, 320);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final heights = <int, double>{for (var i = 0; i < 400; i++) i: 40.0};
      for (var i = 0; i < 400; i += 9) {
        heights[i] = 620.0;
      }
      final data = AnchoredListData<int>(List.generate(400, (i) => i));
      final controller = AnchoredListController();
      final bottomPadding = ValueNotifier<double>(240);
      addTearDown(bottomPadding.dispose);

      await tester.pumpWidget(
        buildResizingSubject(
          data: data,
          controller: controller,
          heights: heights,
          bottomPadding: bottomPadding,
          viewportHeight: 320,
        ),
      );
      await tester.pumpAndSettle();

      controller.position!.jumpTo(controller.position!.maxScrollExtent * 0.6);
      await tester.pumpAndSettle();

      for (var n = 0; n < 6; n++) {
        final id = 400 + n;
        data.insert(0, id);
        heights[id] = 20;
        await tester.pump();
        expect(tester.takeException(), isNull, reason: 'insert $n');

        heights[id] = 600;
        bottomPadding.value = n.isEven ? 90 : 240;
        await tester.pump();
        expect(tester.takeException(), isNull, reason: 'decode $n');
      }
    });
  });
}
