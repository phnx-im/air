// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/app_localizations_en.dart';
import 'package:air/util/time/time_labels.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';

void main() {
  // A Friday, so the weekday tiers below land on named days.
  final now = DateTime(2023, 12, 15, 13, 32, 15);
  final loc = AppLocalizationsEn();

  const formats24 = TimeFormats(
    locale: 'en_US',
    timePattern: 'HH:mm',
    datePattern: 'M/d/yy',
  );
  const formats12 = TimeFormats(
    locale: 'en_US',
    timePattern: 'h:mm a',
    datePattern: 'M/d/yy',
  );
  const formatsDe = TimeFormats(
    locale: 'de_DE',
    timePattern: 'HH:mm',
    datePattern: 'dd.MM.yyyy',
  );

  setUpAll(initializeDateFormatting);

  String messageStamp(DateTime at, {TimeFormats formats = formats24}) =>
      messageStampLabel(at, now: now, formats: formats, loc: loc);

  String chatListStamp(DateTime at, {TimeFormats formats = formats24}) =>
      chatListStampLabel(at, now: now, formats: formats, loc: loc);

  String divider(DateTime at, {TimeFormats formats = formats24}) =>
      dateDividerLabel(at, now: now, formats: formats, loc: loc);

  group('messageStampLabel', () {
    test('reads "Now" under a minute', () {
      expect(messageStamp(now), loc.timestamp_now);
      expect(
        messageStamp(now.subtract(const Duration(seconds: 59))),
        loc.timestamp_now,
      );
    });

    test('counts minutes for the first hour', () {
      expect(messageStamp(now.subtract(const Duration(seconds: 60))), '1m');
      expect(messageStamp(now.subtract(const Duration(minutes: 59))), '59m');
    });

    test('turns over to the clock after an hour', () {
      final at = DateTime(2023, 12, 15, 9, 5);
      expect(messageStamp(at), '09:05');
      expect(messageStamp(at, formats: formats12), '9:05 AM');
    });

    test('carries no day, however old the message is', () {
      // The date dividers above the message supply the day.
      expect(messageStamp(DateTime(2019, 3, 2, 22, 7)), '22:07');
    });
  });

  group('chatListStampLabel', () {
    test('reads "Now" under a minute, then counts minutes', () {
      expect(chatListStamp(now), loc.timestamp_now);
      expect(chatListStamp(now.subtract(const Duration(minutes: 5))), '5m');
    });

    test('shows the clock for earlier today', () {
      expect(chatListStamp(DateTime(2023, 12, 15, 9, 5)), '09:05');
      expect(
        chatListStamp(DateTime(2023, 12, 15, 9, 5), formats: formats12),
        '9:05 AM',
      );
    });

    test('names yesterday', () {
      expect(chatListStamp(DateTime(2023, 12, 14, 23, 55)), loc.date_yesterday);
    });

    test('names the weekday up to six calendar days back', () {
      expect(chatListStamp(DateTime(2023, 12, 12, 8, 0)), 'Tue');
      expect(chatListStamp(DateTime(2023, 12, 9, 8, 0)), 'Sat');
    });

    test('dates a moment seven calendar days back, however few hours ago', () {
      // Six days and twenty hours is under seven times twenty-four, but it is
      // last Friday: naming the weekday would read as today.
      final at = now.subtract(const Duration(days: 6, hours: 20));
      expect(at.day, 8);
      expect(chatListStamp(at), '12/8');
    });

    test('drops the year within the year, keeps it before', () {
      expect(chatListStamp(DateTime(2023, 5, 20, 12, 0)), '5/20');
      expect(chatListStamp(DateTime(2022, 5, 20, 12, 0)), '5/20/22');
    });

    test('follows the platform date pattern', () {
      expect(chatListStamp(DateTime(2023, 5, 20), formats: formatsDe), '20.05');
      expect(
        chatListStamp(DateTime(2022, 5, 20), formats: formatsDe),
        '20.05.2022',
      );
    });
  });

  group('dateDividerLabel', () {
    test('names today and yesterday', () {
      expect(divider(now), loc.date_today);
      expect(divider(DateTime(2023, 12, 15, 0, 0)), loc.date_today);
      expect(divider(DateTime(2023, 12, 14, 23, 55)), loc.date_yesterday);
    });

    test('spells the weekday out within the week', () {
      expect(divider(DateTime(2023, 12, 12, 8, 0)), 'Tuesday');
    });

    test('carries the month and day within the year, the year before', () {
      final thisYear = DateTime(2023, 11, 15, 8, 0);
      final earlier = DateTime(2022, 11, 15, 8, 0);
      expect(divider(thisYear), DateFormat.MMMEd('en_US').format(thisYear));
      expect(divider(earlier), DateFormat.yMMMd('en_US').format(earlier));
    });
  });
}
