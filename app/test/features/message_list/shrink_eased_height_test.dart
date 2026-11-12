// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/message_list/shrink_eased_height.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final duration = Effect.duration(MotionPreset.short);

  late ValueNotifier<double> height;
  late ValueNotifier<bool> hold;
  late double shown;

  setUp(() {
    height = ValueNotifier(100);
    hold = ValueNotifier(false);
  });

  tearDown(() {
    height.dispose();
    hold.dispose();
  });

  Future<void> pumpSubject(WidgetTester tester) => tester.pumpWidget(
    ShrinkEasedHeight(
      height: height,
      hold: hold,
      builder: (context, value) {
        shown = value;
        return const SizedBox.shrink();
      },
    ),
  );

  /// Pumps frames through the whole ease and returns the height in each.
  Future<List<double>> trackEase(WidgetTester tester) async {
    final values = <double>[];
    const frame = Duration(milliseconds: 16);
    for (var t = Duration.zero; t <= duration + frame * 2; t += frame) {
      await tester.pump(frame);
      values.add(shown);
    }
    return values;
  }

  testWidgets('starts at the current height', (tester) async {
    await pumpSubject(tester);

    expect(shown, 100);
  });

  testWidgets('jumps when the height grows', (tester) async {
    await pumpSubject(tester);

    height.value = 140;
    await tester.pump();

    expect(shown, 140);
  });

  testWidgets('eases down when the height shrinks', (tester) async {
    await pumpSubject(tester);

    height.value = 50;
    await tester.pump();
    expect(shown, 100);

    final values = await trackEase(tester);

    expect(values.first, inExclusiveRange(50, 100));
    expect(values.last, 50);
    for (var i = 1; i < values.length; i++) {
      expect(values[i], lessThanOrEqualTo(values[i - 1]));
    }
  });

  testWidgets('waits with a shrink while held', (tester) async {
    await pumpSubject(tester);

    hold.value = true;
    height.value = 50;
    await tester.pump(duration * 2);
    expect(shown, 100);

    hold.value = false;
    await tester.pump();
    expect(shown, 100);
    await tester.pump(duration);

    expect(shown, 50);
  });

  testWidgets('grows at once even while held', (tester) async {
    await pumpSubject(tester);

    hold.value = true;
    height.value = 140;
    await tester.pump();

    expect(shown, 140);
  });

  testWidgets('jumps when the height grows mid ease', (tester) async {
    await pumpSubject(tester);

    height.value = 50;
    await tester.pump();
    await tester.pump(duration ~/ 3);
    expect(shown, inExclusiveRange(50, 100));

    height.value = 120;
    await tester.pump();
    expect(shown, 120);
    await tester.pump(duration);

    expect(shown, 120);
  });

  testWidgets('lands on a further shrink mid ease', (tester) async {
    await pumpSubject(tester);

    height.value = 50;
    await tester.pump();
    await tester.pump(duration ~/ 3);

    height.value = 30;
    await tester.pump(duration);

    expect(shown, 30);
  });
}
