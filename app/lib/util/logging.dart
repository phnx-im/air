// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';
import 'package:air/core/core.dart' as core;

/// Initializes Rust logging
///
/// On Android and iOS, the Rust logs additionally are printed to stdout.
core.LogWriter initRustLogging({required String logFile}) {
  if (Platform.isAndroid || Platform.isIOS) {
    // First initialize printing of Rust logs to stdout; otherwise, the logs from
    // `core.initRustLogging` will be lost.
    core.createLogStream().listen((event) {
      // ignore: avoid_print
      print(
        '${event.time} [Rust] ${event.level.asString} ${event.target}: ${event.msg}',
      );
    });
  }
  return core.initRustLogging(logFile: logFile);
}

/// Initializes Dart logging
///
/// Also configures the format of the logs.
void initDartLogging(core.LogWriter logWriter) {
  // Dart logging
  Logger.root.level = kDebugMode ? Level.FINE : Level.INFO;
  Logger.root.onRecord.listen((record) {
    final utcTime = record.time.toUtc();
    final message =
        '$utcTime [Dart] ${record.level.asString} ${record.loggerName}: ${record.message}';
    // ignore: avoid_print
    print(message);
    logWriter.writeLine(message: message);
  });
}

extension on Level {
  String get asString => switch (this) {
    Level.ALL => 'ALL',
    Level.OFF => 'OFF',
    Level.SHOUT => 'SHOUT',
    Level.SEVERE => 'SEVERE',
    Level.WARNING => ' WARN',
    Level.INFO => ' INFO',
    Level.CONFIG => 'CONFIG',
    Level.FINE => ' FINE',
    Level.FINER => 'FINER',
    Level.FINEST => 'FINEST',
    _ => 'UNKNOWN',
  };
}

extension on core.LogEntryLevel {
  String get asString => switch (this) {
    core.LogEntryLevel.trace => 'TRACE',
    core.LogEntryLevel.debug => 'DEBUG',
    core.LogEntryLevel.info => ' INFO',
    core.LogEntryLevel.warn => ' WARN',
    core.LogEntryLevel.error => 'ERROR',
  };
}
