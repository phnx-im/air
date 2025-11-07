// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';
import 'package:air/chat_list/chat_list_content.dart';

void main() {
  group('formatTimestamp', () {
    final fixedNow = DateTime(
      2023,
      12,
      15,
      13,
      32,
      15,
    ); // Fixed "current" time for testing

    test('returns "Now" for timestamps less than 60 seconds ago', () {
      final timestamp = fixedNow.subtract(const Duration(seconds: 59));
      expect(
        formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
        equals('Now'),
      );
    });

    test('returns minutes for timestamps less than 60 minutes ago', () {
      final timestamp = fixedNow.subtract(const Duration(minutes: 59));
      expect(
        formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
        equals('59m'),
      );
    });

    test('returns hour and minutes for today\'s timestamp', () {
      final timestamp = DateTime(
        fixedNow.year,
        fixedNow.month,
        fixedNow.day,
        11,
        32,
      );
      expect(
        formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
        equals('11:32'),
      );
    });

    test('returns "Yesterday" for timestamps from the previous day', () {
      final yesterday = DateTime(
        fixedNow.year,
        fixedNow.month,
        fixedNow.day - 1,
        15,
        30,
      );
      expect(
        formatTimestamp(yesterday.toIso8601String(), now: fixedNow),
        equals('Yesterday'),
      );
    });

    test(
      'returns day of the week for timestamps less than 7 days ago but not today or yesterday',
      () {
        final timestamp = fixedNow.subtract(const Duration(days: 3));
        expect(
          formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
          equals(DateFormat('E').format(timestamp)),
        );
      },
    );

    test('returns correct day and month for timestamps earlier this year', () {
      final timestamp = DateTime(
        fixedNow.year,
        fixedNow.month - 1,
        fixedNow.day,
      );
      expect(
        formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
        equals(DateFormat('dd.MM').format(timestamp)),
      );
    });

    test('returns day, month and year for timestamps from previous years', () {
      final timestamp = DateTime(
        fixedNow.year - 1,
        fixedNow.month,
        fixedNow.day,
      );
      expect(
        formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
        equals(DateFormat('dd.MM.yy').format(timestamp)),
      );
    });

    test('handles timestamps on New Year\'s Eve correctly', () {
      final timestamp = DateTime(fixedNow.year - 1, 12, 31, 23, 59);
      expect(
        formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
        equals('31.12.${(fixedNow.year - 1).toString().substring(2)}'),
      );
    });

    test('handles timestamps exactly at the start of today', () {
      final timestamp = DateTime(
        fixedNow.year,
        fixedNow.month,
        fixedNow.day,
        0,
        0,
        0,
      );
      expect(
        formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
        equals('00:00'),
      );
    });

    test('handles timestamps exactly one year ago', () {
      final timestamp = DateTime(
        fixedNow.year - 1,
        fixedNow.month,
        fixedNow.day,
      );
      expect(
        formatTimestamp(timestamp.toIso8601String(), now: fixedNow),
        equals(DateFormat('dd.MM.yy').format(timestamp)),
      );
    });
  });
}
