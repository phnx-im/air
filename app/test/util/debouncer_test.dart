// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/util/debouncer.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

const _delay = Duration(seconds: 1);

void main() {
  group('Debouncer', () {
    late List<String> runs;
    late Debouncer debouncer;

    setUp(() {
      runs = [];
      debouncer = Debouncer(delay: _delay);
    });

    VoidCallback record(String label) =>
        () => runs.add(label);

    // `testWidgets` rather than `test`: it runs the body under the fake clock,
    // so `pump` moves the timer forward without the test waiting out the delay.
    testWidgets('runs the action once the delay elapses', (tester) async {
      debouncer.run(record('a'));

      await tester.pump(_delay - const Duration(milliseconds: 1));
      expect(runs, isEmpty);

      await tester.pump(const Duration(milliseconds: 1));
      expect(runs, ['a']);
    });

    testWidgets('keeps only the last action of a burst', (tester) async {
      debouncer.run(record('a'));
      await tester.pump(const Duration(milliseconds: 500));
      debouncer.run(record('b'));

      await tester.pump(_delay);
      expect(runs, ['b']);
    });

    testWidgets('cancel drops the pending action', (tester) async {
      debouncer.run(record('a'));
      await tester.pump(const Duration(milliseconds: 500));
      debouncer.cancel();

      await tester.pump(_delay);
      expect(runs, isEmpty);
    });

    testWidgets('cancel is a no-op when nothing is pending', (tester) async {
      debouncer.cancel();

      await tester.pump(_delay);
      expect(runs, isEmpty);
    });

    testWidgets('a flush after a cancel has nothing left to run', (
      tester,
    ) async {
      debouncer.run(record('a'));
      debouncer.cancel();
      debouncer.flush();

      await tester.pump(_delay);
      expect(runs, isEmpty);
    });

    testWidgets('schedules again after a cancel', (tester) async {
      debouncer.run(record('a'));
      debouncer.cancel();
      debouncer.run(record('b'));

      await tester.pump(_delay);
      expect(runs, ['b']);
    });

    testWidgets('flush runs the pending action right away, and only once', (
      tester,
    ) async {
      debouncer.run(record('a'));
      await tester.pump(const Duration(milliseconds: 100));

      debouncer.flush();
      expect(runs, ['a']);

      await tester.pump(_delay);
      expect(runs, ['a']);
    });

    testWidgets('flush is a no-op when nothing is pending', (tester) async {
      debouncer.flush();

      await tester.pump(_delay);
      expect(runs, isEmpty);
    });

    testWidgets('flush after the delay does not run the action twice', (
      tester,
    ) async {
      debouncer.run(record('a'));
      await tester.pump(_delay);
      expect(runs, ['a']);

      debouncer.flush();
      expect(runs, ['a']);
    });

    testWidgets('dispose drops the pending action', (tester) async {
      debouncer.run(record('a'));
      debouncer.dispose();

      await tester.pump(_delay);
      expect(runs, isEmpty);
    });
  });
}
