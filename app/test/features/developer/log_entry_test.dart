// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/developer/log_entry.dart';
import 'package:flutter_test/flutter_test.dart';

/// The buffer as `readAppLogs` hands it over: the written lines, reversed.
String buffer(List<String> written) => '${written.reversed.join('\n')}\n';

void main() {
  group('parseLogs', () {
    test('reads a Rust line', () {
      final entries = parseLogs(
        buffer([
          '2026-08-08T14:22:01.334567Z  INFO aircoreclient::groups: '
              'joined group members=3',
        ]),
      );

      expect(entries, hasLength(1));
      final entry = entries.single;
      expect(entry.time, DateTime.utc(2026, 8, 8, 14, 22, 1, 334, 567));
      expect(entry.level, LogLevel.info);
      expect(entry.source, LogSource.rust);
      expect(entry.target, 'aircoreclient::groups');
      expect(entry.message, 'joined group members=3');
      expect(entry.detail, isNull);
    });

    test('reads a Dart line', () {
      final entries = parseLogs(
        buffer(['2026-08-08 14:22:01.334Z [Dart]  WARN AppRouter: no route']),
      );

      final entry = entries.single;
      expect(entry.time, DateTime.utc(2026, 8, 8, 14, 22, 1, 334));
      expect(entry.level, LogLevel.warn);
      expect(entry.source, LogSource.dart);
      expect(entry.target, 'AppRouter');
      expect(entry.message, 'no route');
    });

    test('maps the levels the two sides write', () {
      final entries = parseLogs(
        buffer([
          '2026-08-08T14:22:01.000000Z TRACE a: t',
          '2026-08-08T14:22:02.000000Z DEBUG a: d',
          '2026-08-08T14:22:03.000000Z ERROR a: e',
          '2026-08-08 14:22:04.000Z [Dart] FINEST b: t',
          '2026-08-08 14:22:05.000Z [Dart]  FINE b: d',
          '2026-08-08 14:22:06.000Z [Dart] SEVERE b: e',
          '2026-08-08 14:22:07.000Z [Dart] CUSTOM b: c',
        ]),
      );

      expect(entries.map((entry) => entry.level).toList().reversed, [
        LogLevel.trace,
        LogLevel.debug,
        LogLevel.error,
        LogLevel.trace,
        LogLevel.debug,
        LogLevel.error,
        LogLevel.info,
      ]);
    });

    // The buffer arrives reversed, so getting this wrong shows the log
    // backwards.
    test('returns the newest record first', () {
      final entries = parseLogs(
        buffer([
          '2026-08-08T14:22:01.000000Z  INFO a: first',
          '2026-08-08T14:22:02.000000Z  INFO a: second',
          '2026-08-08T14:22:03.000000Z  INFO a: third',
        ]),
      );

      expect(entries.map((entry) => entry.message), [
        'third',
        'second',
        'first',
      ]);
    });

    // A Dart record writes its error and stack trace as lines of their own,
    // which the reversal scrambles.
    test('collects the lines that continue a record', () {
      final entries = parseLogs(
        buffer([
          '2026-08-08 14:22:01.334Z [Dart] SEVERE Chat: send failed',
          'Error: Bad state: no group',
          'StackTrace: #0      Chat.send (chat.dart:12)',
          '#1      _rootRun (zone.dart:1399)',
          '2026-08-08T14:22:02.000000Z  INFO aircoreclient: next',
        ]),
      );

      expect(entries.map((entry) => entry.message), ['next', 'send failed']);
      expect(entries.last.detail, '''
Error: Bad state: no group
StackTrace: #0      Chat.send (chat.dart:12)
#1      _rootRun (zone.dart:1399)''');
      expect(entries.first.detail, isNull);
    });

    // The ring buffer wraps mid-line, so the oldest line is usually half a
    // record with nothing to continue.
    test('drops the fragment the buffer wrapped through', () {
      final entries = parseLogs(
        buffer([
          'client::groups: half a line from before the buffer wrapped',
          '2026-08-08T14:22:01.000000Z  INFO aircoreclient: first whole record',
        ]),
      );

      expect(entries.map((entry) => entry.message), ['first whole record']);
    });

    test('keeps a message that carries its own colon', () {
      final entries = parseLogs(
        buffer([
          '2026-08-08T14:22:01.000000Z ERROR aircoreclient::qs: '
              'connect failed: refused',
        ]),
      );

      expect(entries.single.target, 'aircoreclient::qs');
      expect(entries.single.message, 'connect failed: refused');
    });

    // Span context is written ahead of the target. The record has no field for
    // it, so it stays on the message.
    test('keeps span context ahead of the message', () {
      final entries = parseLogs(
        buffer([
          '2026-08-08T14:22:01.000000Z  INFO fetch{id=7}: '
              'aircoreclient::qs: dequeued',
        ]),
      );

      expect(entries.single.target, 'aircoreclient::qs');
      expect(entries.single.message, 'fetch{id=7}: dequeued');
    });

    test('reads an empty buffer as no records', () {
      expect(parseLogs(''), isEmpty);
      expect(parseLogs('\n\n'), isEmpty);
    });
  });

  group('groupByDay', () {
    // Local time, so the expected day holds in whatever zone the test runs in.
    LogEntry at(DateTime time) => LogEntry(
      time: time,
      level: LogLevel.info,
      source: LogSource.rust,
      target: 'aircoreclient',
      message: '${time.hour}',
    );

    test('keeps a day of records together, newest day first', () {
      final days = groupByDay([
        at(DateTime(2026, 8, 9, 9, 5)),
        at(DateTime(2026, 8, 8, 23, 59)),
        at(DateTime(2026, 8, 8, 0, 1)),
      ]);

      expect(days.map((day) => day.date), [
        DateTime(2026, 8, 9),
        DateTime(2026, 8, 8),
      ]);
      expect(days.first.entries, hasLength(1));
      expect(days.last.entries.map((entry) => entry.message), ['23', '0']);
    });

    // Records a minute apart across midnight are the pair a same-hour
    // comparison would merge.
    test('breaks on the day, not on the clock', () {
      final days = groupByDay([
        at(DateTime(2026, 8, 9, 0, 0, 30)),
        at(DateTime(2026, 8, 8, 23, 59, 30)),
      ]);

      expect(days, hasLength(2));
    });

    test('opens a day of its own for a record that arrives out of order', () {
      final days = groupByDay([
        at(DateTime(2026, 8, 9, 9, 0)),
        at(DateTime(2026, 8, 8, 9, 0)),
        at(DateTime(2026, 8, 9, 8, 0)),
      ]);

      expect(days.map((day) => day.date), [
        DateTime(2026, 8, 9),
        DateTime(2026, 8, 8),
        DateTime(2026, 8, 9),
      ]);
    });

    test('reads no records as no days', () {
      expect(groupByDay([]), isEmpty);
    });
  });

  group('LogFilter', () {
    LogEntry entry({
      LogLevel level = LogLevel.info,
      String target = 'aircoreclient::qs',
      String message = 'dequeued 3 messages',
    }) => LogEntry(
      time: DateTime.utc(2026, 8, 8, 14, 22, 1),
      level: level,
      source: LogSource.rust,
      target: target,
      message: message,
    );

    test('passes everything by default', () {
      expect(const LogFilter().matches(entry(level: LogLevel.trace)), isTrue);
    });

    test('drops what is quieter than the threshold', () {
      const filter = LogFilter(threshold: LogLevel.warn);

      expect(filter.matches(entry(level: LogLevel.info)), isFalse);
      expect(filter.matches(entry(level: LogLevel.warn)), isTrue);
      expect(filter.matches(entry(level: LogLevel.error)), isTrue);
    });

    test('matches the query against the message and the target', () {
      expect(const LogFilter(query: 'DEQUEUED').matches(entry()), isTrue);
      expect(const LogFilter(query: 'qs').matches(entry()), isTrue);
      expect(const LogFilter(query: 'ds').matches(entry()), isFalse);
    });

    test('isolates one target exactly', () {
      const filter = LogFilter(target: 'aircoreclient::qs');

      expect(filter.matches(entry()), isTrue);
      expect(
        filter.matches(entry(target: 'aircoreclient::qs::inner')),
        isFalse,
      );
    });

    // The three narrow together, so a record has to clear all of them.
    test('narrows on every axis at once', () {
      const filter = LogFilter(
        threshold: LogLevel.warn,
        query: 'dequeued',
        target: 'aircoreclient::qs',
      );

      expect(filter.matches(entry(level: LogLevel.error)), isTrue);
      expect(filter.matches(entry(level: LogLevel.info)), isFalse);
      expect(
        filter.matches(entry(level: LogLevel.error, message: 'enqueued')),
        isFalse,
      );
      expect(
        filter.matches(entry(level: LogLevel.error, target: 'aircoreclient')),
        isFalse,
      );
    });

    // Null means "leave it", so clearing needs a flag of its own.
    test('clears the target only when asked to', () {
      const filter = LogFilter(target: 'aircoreclient::qs');

      expect(filter.copyWith(query: 'x').target, 'aircoreclient::qs');
      expect(filter.copyWith(clearTarget: true).target, isNull);
    });
  });
}
