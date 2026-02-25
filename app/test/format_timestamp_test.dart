// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter_test/flutter_test.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';
import 'package:air/chat_list/chat_list_content.dart';
import 'package:air/l10n/app_localizations_en.dart';

void main() {
  final fixedNow = DateTime(2023, 12, 15, 13, 32, 15);
  const locale = 'en_US';
  const timePattern12h = 'h:mm a';
  const timePattern24h = 'HH:mm';
  const datePatternUS = 'M/d/yy';
  final loc = AppLocalizationsEn();

  group('classifyTimestamp', () {
    test('returns now for < 60 seconds ago', () {
      final ts = fixedNow.subtract(const Duration(seconds: 59));
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.now);
    });

    test('returns now for 0 seconds ago', () {
      expect(classifyTimestamp(fixedNow, now: fixedNow), TimestampCategory.now);
    });

    test('returns minutes for < 60 minutes ago', () {
      final ts = fixedNow.subtract(const Duration(minutes: 59));
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.minutes);
    });

    test('returns minutes at exactly 60 seconds ago', () {
      final ts = fixedNow.subtract(const Duration(seconds: 60));
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.minutes);
    });

    test('returns today for earlier today beyond 60 minutes', () {
      final ts = DateTime(fixedNow.year, fixedNow.month, fixedNow.day, 0, 0, 0);
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.today);
    });

    test('returns yesterday for previous day', () {
      final ts = DateTime(
        fixedNow.year,
        fixedNow.month,
        fixedNow.day - 1,
        15,
        30,
      );
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.yesterday);
    });

    test('returns thisWeek for 3 days ago', () {
      final ts = fixedNow.subtract(const Duration(days: 3));
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.thisWeek);
    });

    test('returns thisYear for same year beyond 7 days', () {
      final ts = DateTime(fixedNow.year, fixedNow.month - 1, fixedNow.day);
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.thisYear);
    });

    test('returns older for previous year', () {
      final ts = DateTime(fixedNow.year - 1, fixedNow.month, fixedNow.day);
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.older);
    });

    test('returns older for New Year\'s Eve of previous year', () {
      final ts = DateTime(fixedNow.year - 1, 12, 31, 23, 59);
      expect(classifyTimestamp(ts, now: fixedNow), TimestampCategory.older);
    });
  });

  group('formatTimestamp', () {
    test('returns "Now" for now category', () {
      final ts = fixedNow.subtract(const Duration(seconds: 59));
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.now,
          timePattern: timePattern12h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals(loc.timestamp_now),
      );
    });

    test('returns minutes for minutes category', () {
      final ts = fixedNow.subtract(const Duration(minutes: 59));
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.minutes,
          timePattern: timePattern12h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals(loc.timestamp_minutesAgo(59)),
      );
    });

    test('returns formatted time for today category', () {
      final ts = DateTime(fixedNow.year, fixedNow.month, fixedNow.day, 11, 32);
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.today,
          timePattern: timePattern12h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals(DateFormat(timePattern12h).format(ts)),
      );
    });

    test('uses 24h time pattern when provided', () {
      final ts = DateTime(fixedNow.year, fixedNow.month, fixedNow.day, 9, 45);
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.today,
          timePattern: timePattern24h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals('09:45'),
      );
    });

    test('returns "Yesterday" for yesterday category', () {
      final ts = DateTime(
        fixedNow.year,
        fixedNow.month,
        fixedNow.day - 1,
        15,
        30,
      );
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.yesterday,
          timePattern: timePattern12h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals(loc.timestamp_yesterday),
      );
    });

    test('returns day of week for thisWeek category', () {
      final ts = fixedNow.subtract(const Duration(days: 3));
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.thisWeek,
          timePattern: timePattern12h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals(DateFormat.E(locale).format(ts)),
      );
    });

    test('returns date without year for thisYear category', () {
      final ts = DateTime(fixedNow.year, fixedNow.month - 1, fixedNow.day);
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.thisYear,
          timePattern: timePattern12h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals(DateFormat('M/d').format(ts)),
      );
    });

    test('returns full date for older category', () {
      final ts = DateTime(fixedNow.year - 1, fixedNow.month, fixedNow.day);
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.older,
          timePattern: timePattern12h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals(DateFormat(datePatternUS).format(ts)),
      );
    });

    test('handles start of today with today category', () {
      final ts = DateTime(fixedNow.year, fixedNow.month, fixedNow.day, 0, 0, 0);
      expect(
        formatTimestamp(
          ts,
          loc,
          TimestampCategory.today,
          timePattern: timePattern12h,
          datePattern: datePatternUS,
          locale: locale,
          now: fixedNow,
        ),
        equals(DateFormat(timePattern12h).format(ts)),
      );
    });

    test('respects custom date pattern for thisYear', () async {
      await initializeDateFormatting('de_DE');
      const deDatePattern = 'dd.MM.yy';
      final ts = DateTime(fixedNow.year, fixedNow.month - 1, fixedNow.day);
      final usResult = formatTimestamp(
        ts,
        loc,
        TimestampCategory.thisYear,
        timePattern: timePattern12h,
        datePattern: datePatternUS,
        locale: locale,
        now: fixedNow,
      );
      final deResult = formatTimestamp(
        ts,
        loc,
        TimestampCategory.thisYear,
        timePattern: timePattern12h,
        datePattern: deDatePattern,
        locale: 'de_DE',
        now: fixedNow,
      );
      expect(usResult, equals(DateFormat('M/d').format(ts)));
      expect(deResult, equals(DateFormat('dd.MM').format(ts)));
    });
  });
}
