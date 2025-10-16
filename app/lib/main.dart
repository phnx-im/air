// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/core/frb_generated.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/util/logging.dart';
import 'package:air/util/platform.dart';
import 'package:flutter/material.dart';
import 'package:iconoir_flutter/regular/warning_circle.dart';
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

void showErrorBanner(BuildContext context, String errorDescription) {
  ScaffoldMessenger.of(context).showMaterialBanner(
    MaterialBanner(
      backgroundColor: CustomColorScheme.of(context).function.danger,
      leading: WarningCircle(
        width: 32,
        color: CustomColorScheme.of(context).function.white,
      ),
      padding: const EdgeInsets.all(20),
      content: Text(errorDescription),
      actions: [
        TextButton(
          child: Text(
            'OK',
            style: TextStyle(
              color: CustomColorScheme.of(context).function.white,
            ),
          ),
          onPressed: () {
            ScaffoldMessenger.of(context).hideCurrentMaterialBanner();
          },
        ),
      ],
    ),
  );
}
