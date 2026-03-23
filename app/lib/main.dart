// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/core/frb_generated.dart' show RustLib;
import 'package:air/util/logging.dart';
import 'package:air/util/platform.dart';
import 'package:flutter/material.dart';
import 'package:path/path.dart' as p;

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  await RustLib.init();

  final cacheDir = await getCacheDirectory();
  final logFile = p.join(cacheDir, 'app.log');

  final logWriter = initRustLogging(logFile: logFile);
  initDartLogging(logWriter);

  runApp(const App());
}
