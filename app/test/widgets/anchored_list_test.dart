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
}
