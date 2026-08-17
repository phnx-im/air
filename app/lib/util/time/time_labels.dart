// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:flutter/widgets.dart';
import 'package:intl/intl.dart';
import 'package:system_date_time_format/system_date_time_format.dart';

/// The conventions a time label is written in: the locale that names its
/// weekdays and months, and the clock and date patterns the platform itself is
/// set to.
///
/// Reading the patterns from the platform rather than from the locale alone is
/// what makes a 24-hour device show `14:32` where a 12-hour one shows
/// `2:32 PM`, in the same locale.
@immutable
class TimeFormats {
  const TimeFormats({
    required this.locale,
    required this.timePattern,
    required this.datePattern,
  });

  /// Resolves the ambient locale and the platform's own patterns.
  ///
  /// The patterns come from the [SDTFScope] the app installs at its root. The
  /// locale's own short forms stand in where the scope reports no pattern, and
  /// equally where there is no scope at all -- a widget test pumping a subtree
  /// of its own.
  factory TimeFormats.of(BuildContext context) {
    final locale = Localizations.localeOf(context).toString();
    // Asked for by widget: the scope's own accessor throws where it is absent,
    // rather than reporting it, and the widget is the only public way to look.
    final patterns = context.findAncestorWidgetOfExactType<SDTFScope>() != null
        ? SystemDateTimeFormat.of(context)
        : null;
    return TimeFormats(
      locale: locale,
      timePattern:
          patterns?.timePattern ??
          _cached('jm|$locale', () => DateFormat.jm(locale)).pattern!,
      datePattern:
          patterns?.datePattern ??
          _cached('yMd|$locale', () => DateFormat.yMd(locale)).pattern!,
    );
  }

  final String locale;

  /// Short clock pattern, e.g. `HH:mm` or `h:mm a`.
  final String timePattern;

  /// Numeric date pattern, e.g. `dd.MM.yyyy` or `M/d/yy`.
  final String datePattern;

  /// The clock alone: `14:32`, or `2:32 PM` on a 12-hour device.
  String clock(DateTime at) =>
      _cached('c|$timePattern', () => DateFormat(timePattern)).format(at);

  /// Abbreviated weekday, e.g. `Mon`.
  String weekdayShort(DateTime at) =>
      _cached('E|$locale', () => DateFormat.E(locale)).format(at);

  /// Full weekday, e.g. `Monday`.
  String weekdayLong(DateTime at) =>
      _cached('EEEE|$locale', () => DateFormat.EEEE(locale)).format(at);

  /// Numeric date, e.g. `5/20/26`.
  String date(DateTime at) =>
      _cached('d|$datePattern', () => DateFormat(datePattern)).format(at);

  /// Numeric date with the year dropped, e.g. `5/20`.
  String dateWithoutYear(DateTime at) => _cached(
    'dy|$datePattern',
    () => DateFormat(_withoutYear(datePattern)),
  ).format(at);

  /// Weekday, month and day, e.g. `Wed, May 20`.
  String weekdayMonthDay(DateTime at) =>
      _cached('MMMEd|$locale', () => DateFormat.MMMEd(locale)).format(at);

  /// Month, day and year, e.g. `May 20, 2026`.
  String monthDayYear(DateTime at) =>
      _cached('yMMMd|$locale', () => DateFormat.yMMMd(locale)).format(at);

  @override
  bool operator ==(Object other) =>
      other is TimeFormats &&
      other.locale == locale &&
      other.timePattern == timePattern &&
      other.datePattern == datePattern;

  @override
  int get hashCode => Object.hash(locale, timePattern, datePattern);
}

/// [DateFormat] re-parses its pattern on every fresh instance, and these are
/// asked for once per visible row on every clock tick, so instances are kept.
/// Keyed by pattern and locale; the ambient default locale is appended for
/// the pattern-only constructors, which resolve their symbols through it.
final _dateFormats = <String, DateFormat>{};

DateFormat _cached(String key, DateFormat Function() create) =>
    _dateFormats['$key|${Intl.getCurrentLocale()}'] ??= create();

final _yearInPattern = RegExp(r"[/.\-,\s]*[yY]+[/.\-,\s]*");

/// Strips the year and any separator left stranded beside it, so `M/d/yy`
/// reads `M/d` and `dd.MM.yyyy` reads `dd.MM`.
String _withoutYear(String pattern) =>
    pattern.replaceAll(_yearInPattern, '').trim();

/// A moment's distance from [now], in the terms the labels below distinguish.
/// Calendar days, not 24-hour windows: a message sent late yesterday reads
/// "Yesterday" all of today. First match wins.
enum _Tier {
  /// Under a minute old.
  now,

  /// Under an hour old.
  minutes,

  /// Earlier today.
  today,
  yesterday,

  /// Two to six calendar days back, where a weekday name is still unambiguous.
  thisWeek,
  thisYear,
  older,
}

_Tier _tier(DateTime at, DateTime now) {
  final since = now.difference(at);
  if (since.inSeconds < 60) return _Tier.now;
  if (since.inMinutes < 60) return _Tier.minutes;
  // We use UTC to avoid problems with DST
  final today = DateTime.utc(now.year, now.month, now.day);
  final day = DateTime.utc(at.year, at.month, at.day);
  final days = today.difference(day).inDays;
  if (days <= 0) return _Tier.today;
  if (days == 1) return _Tier.yesterday;
  if (days <= 6) return _Tier.thisWeek;
  if (at.year == now.year) return _Tier.thisYear;
  return _Tier.older;
}

/// The stamp under a message bubble: the elapsed minutes for the first hour,
/// the clock time after that.
///
/// The day a message was sent comes from the date dividers around it, so the
/// stamp never repeats it.
String messageStampLabel(
  DateTime at, {
  required DateTime now,
  required TimeFormats formats,
  required AppLocalizations loc,
}) {
  final local = at.toLocal();
  return switch (_tier(local, now)) {
    _Tier.now => loc.timestamp_now,
    _Tier.minutes => loc.timestamp_minutesAgo(now.difference(local).inMinutes),
    _ => formats.clock(local),
  };
}

/// The "last activity" stamp beside a chat-list row's title, which stands alone
/// and so carries the day itself once the moment is no longer today.
String chatListStampLabel(
  DateTime at, {
  required DateTime now,
  required TimeFormats formats,
  required AppLocalizations loc,
}) {
  final local = at.toLocal();
  return switch (_tier(local, now)) {
    _Tier.now => loc.timestamp_now,
    _Tier.minutes => loc.timestamp_minutesAgo(now.difference(local).inMinutes),
    _Tier.today => formats.clock(local),
    _Tier.yesterday => loc.date_yesterday,
    _Tier.thisWeek => formats.weekdayShort(local),
    _Tier.thisYear => formats.dateWithoutYear(local),
    _Tier.older => formats.date(local),
  };
}

/// The date pill separating one day of messages from the next. Always a day,
/// never a clock: the messages under it carry the time.
String dateDividerLabel(
  DateTime at, {
  required DateTime now,
  required TimeFormats formats,
  required AppLocalizations loc,
}) {
  final local = at.toLocal();
  return switch (_tier(local, now)) {
    _Tier.now || _Tier.minutes || _Tier.today => loc.date_today,
    _Tier.yesterday => loc.date_yesterday,
    _Tier.thisWeek => formats.weekdayLong(local),
    _Tier.thisYear => formats.weekdayMonthDay(local),
    _Tier.older => formats.monthDayYear(local),
  };
}
