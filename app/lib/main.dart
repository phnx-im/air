// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/core/frb_generated.dart' show RustLib;
import 'package:air/platform/logging.dart';
import 'package:air/platform/method_channel.dart';
import 'package:air/share/share.dart' as share;
import 'package:flutter/material.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();

  await initializeDateFormatting();
  await RustLib.init();

  final cacheDir = await getCacheDirectory();
  final logFile = p.join(cacheDir, 'app.log');

  final logWriter = initRustLogging(logFile: logFile);
  initDartLogging(logWriter);

  runApp(App(clientRecordId: _clientRecordIdArg(args)));
}

/// Parses `--client-record-id <uuid>` (or `--client-record-id=<uuid>`) from
/// the command line arguments.
///
/// When given, the app opens this client record at startup instead of the
/// default one. Only supported on desktop platforms, where the runner forwards
/// the process arguments to `main`.
UuidValue? _clientRecordIdArg(List<String> args) {
  const flag = '--client-record-id';
  for (var i = 0; i < args.length; i++) {
    final arg = args[i];
    if (arg == flag && i + 1 < args.length) {
      return UuidValue.withValidation(args[i + 1]);
    }
    if (arg.startsWith('$flag=')) {
      return UuidValue.withValidation(arg.substring(flag.length + 1));
    }
  }
  return null;
}

/// Entrypoint of the share UI, hosted by the iOS share extension.
@pragma('vm:entry-point')
void shareMain() => share.shareMain();
