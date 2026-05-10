// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/widgets/anchored_list/anchored_list.dart';
import 'package:air/widgets/anchored_list/controller.dart';
import 'package:air/widgets/anchored_list/data.dart';
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

      controller.goToId(20);
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
        controller.goToId(20);
        await tester.pumpAndSettle();
        controller.position!.jumpTo(controller.position!.pixels - 240);
        await tester.pump();

        final viewportTopY = tester
            .getTopLeft(find.byType(AnchoredList<int>))
            .dy;
        final clippedTop = (await rectOf(tester, 18)).top - viewportTopY;
        expect(clippedTop, lessThan(topPadding));
        expect(clippedTop, greaterThan(0));

        controller.goToId(18);
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
      controller.goToId(20);
      await tester.pumpAndSettle();

      final pixelsBefore = controller.position!.pixels;
      final rectBefore = await rectOf(tester, 18);

      controller.goToId(18);
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
        controller.goToId(target);
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
        controller.goToId(target);
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
}
