// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The log buffer read as records rather than as one string.
///
/// We parse in Dart because `readAppLogs` already hands the whole buffer
/// across, so a viewer costs no bridge change.
library;

/// The level a record was written at. Quietest first, so a threshold filter
/// compares indices.
enum LogLevel { trace, debug, info, warn, error }

/// Which side of the bridge wrote the record. Both write into the same buffer,
/// each in its own format.
enum LogSource { rust, dart }

/// One record in the log buffer.
class LogEntry {
  const LogEntry({
    required this.time,
    required this.level,
    required this.source,
    required this.target,
    required this.message,
    this.detail,
  });

  final DateTime time;
  final LogLevel level;
  final LogSource source;

  /// The Rust module path, or the Dart logger name.
  final String target;

  final String message;

  /// Whatever followed the record on lines of its own: an error, a stack
  /// trace, a panic payload.
  final String? detail;
}

/// What a viewer shows of the records it holds. All three narrow together.
class LogFilter {
  const LogFilter({
    this.threshold = LogLevel.trace,
    this.query = '',
    this.target,
  });

  /// The quietest level still shown. [LogLevel.trace] shows everything.
  final LogLevel threshold;

  /// Free text over the message and the target, matched case-insensitively.
  final String query;

  /// The one target shown, or null for all.
  final String? target;

  bool matches(LogEntry entry) {
    if (entry.level.index < threshold.index) return false;
    if (target != null && entry.target != target) return false;
    if (query.isEmpty) return true;

    final needle = query.toLowerCase();
    return entry.message.toLowerCase().contains(needle) ||
        entry.target.toLowerCase().contains(needle);
  }

  /// [target] is cleared through [clearTarget], since null means "leave it".
  LogFilter copyWith({
    LogLevel? threshold,
    String? query,
    String? target,
    bool clearTarget = false,
  }) => LogFilter(
    threshold: threshold ?? this.threshold,
    query: query ?? this.query,
    target: clearTarget ? null : target ?? this.target,
  );

  @override
  bool operator ==(Object other) =>
      other is LogFilter &&
      other.threshold == threshold &&
      other.query == query &&
      other.target == target;

  @override
  int get hashCode => Object.hash(threshold, query, target);
}

/// The records of one local calendar day, newest first.
///
/// We group rather than flatten with marker rows, so a viewer can pin a day's
/// header for as long as that day's records are on screen.
class LogDay {
  LogDay({required this.date, required this.entries});

  /// Local midnight of the day the records fall on.
  final DateTime date;

  final List<LogEntry> entries;
}

/// Splits [entries], newest first, into local calendar days, newest first.
///
/// We compare in local time: the buffer stores UTC, but the day being looked
/// for is the one the device was showing. Only adjacent records are grouped,
/// so out-of-order records open a day of their own.
List<LogDay> groupByDay(List<LogEntry> entries) {
  final days = <LogDay>[];

  for (final entry in entries) {
    final local = entry.time.toLocal();
    final date = DateTime(local.year, local.month, local.day);
    if (days.isEmpty || days.last.date != date) {
      days.add(LogDay(date: date, entries: [entry]));
    } else {
      days.last.entries.add(entry);
    }
  }

  return days;
}

/// Parses [raw] into records, newest first.
///
/// `read_logs_from_buffer` hands the lines back reversed. We reverse them to
/// recover write order, so a record can collect the lines continuing it, then
/// reverse the records back for display.
List<LogEntry> parseLogs(String raw) {
  final records = <_Record>[];

  for (final line in raw.split('\n').reversed) {
    final record = _parseLine(line);
    if (record != null) {
      records.add(record);
    } else if (records.isNotEmpty) {
      records.last.detail.add(line);
    }
    // A continuation with nothing to continue is the line the ring buffer
    // wrapped through, and is dropped.
  }

  return [for (final record in records.reversed) record.build()];
}

/// A Rust line, as `tracing_subscriber::fmt` writes it:
/// `2026-08-08T14:22:01.334567Z  INFO aircoreclient::groups: msg key=value`
final _rustLine = RegExp(
  r'^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)\s+'
  r'(TRACE|DEBUG|INFO|WARN|ERROR)\s+(.*)$',
);

/// A Dart line, as `platform/logging.dart` builds it:
/// `2026-08-08 14:22:01.334Z [Dart]  INFO AppRouter: msg`
final _dartLine = RegExp(
  r'^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?Z) \[Dart\]\s+(\S+)\s+(.*)$',
);

/// The module path a Rust line names, and the message after it. The leading
/// group takes any span context written ahead of the target, hence the
/// non-greedy match rather than one anchored at the head of the line.
final _rustTarget = RegExp(
  r'^(.*?)([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z0-9_]+)*): (.*)$',
);

_Record? _parseLine(String line) {
  if (_rustLine.firstMatch(line) case final match?) {
    final time = DateTime.tryParse(match.group(1)!);
    if (time != null) {
      final (target, message) = _splitRustTarget(match.group(3)!);
      return _Record(
        time: time,
        level: _rustLevel(match.group(2)!),
        source: LogSource.rust,
        target: target,
        message: message,
      );
    }
  }

  if (_dartLine.firstMatch(line) case final match?) {
    final time = DateTime.tryParse(match.group(1)!);
    if (time != null) {
      final (target, message) = _splitDartTarget(match.group(3)!);
      return _Record(
        time: time,
        level: _dartLevel(match.group(2)!),
        source: LogSource.dart,
        target: target,
        message: message,
      );
    }
  }

  return null;
}

/// Splits `[span{field=1}: ]target: message` into its target and the rest.
///
/// Span context stays on the front of the message. The record has no field for
/// it, and we would rather show it in an odd place than drop it.
(String, String) _splitRustTarget(String rest) {
  final match = _rustTarget.firstMatch(rest);
  if (match == null) return ('', rest);
  return (match.group(2)!, '${match.group(1)!}${match.group(3)!}');
}

(String, String) _splitDartTarget(String rest) {
  final separator = rest.indexOf(': ');
  if (separator < 0) return ('', rest);
  return (rest.substring(0, separator), rest.substring(separator + 2));
}

LogLevel _rustLevel(String token) => switch (token) {
  'TRACE' => LogLevel.trace,
  'DEBUG' => LogLevel.debug,
  'WARN' => LogLevel.warn,
  'ERROR' => LogLevel.error,
  _ => LogLevel.info,
};

/// The `logging` package has more levels than `tracing`, so several map onto
/// one. An unrecognized token still starts a record, the `[Dart]` marker ahead
/// of it already says the line is one.
LogLevel _dartLevel(String token) => switch (token) {
  'FINEST' || 'FINER' => LogLevel.trace,
  'FINE' || 'CONFIG' => LogLevel.debug,
  'WARN' || 'WARNING' => LogLevel.warn,
  'SEVERE' || 'SHOUT' => LogLevel.error,
  _ => LogLevel.info,
};

/// A record still collecting the lines that continue it.
class _Record {
  _Record({
    required this.time,
    required this.level,
    required this.source,
    required this.target,
    required this.message,
  });

  final DateTime time;
  final LogLevel level;
  final LogSource source;
  final String target;
  final String message;
  final List<String> detail = [];

  LogEntry build() {
    final detail = this.detail.join('\n').trim();
    return LogEntry(
      time: time,
      level: level,
      source: source,
      target: target,
      message: message,
      detail: detail.isEmpty ? null : detail,
    );
  }
}
