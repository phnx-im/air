// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// ignore: depend_on_referenced_packages
import 'package:flutter_rust_bridge_hooks/flutter_rust_bridge_hooks.dart';

void main(List<String> args) async {
  await build(args, (input, output) async {
    await const FlutterRustBridgeNativeAssetsBuilder(
      cratePath: '../applogic',
      // Must match `dart_output: lib/core` in flutter_rust_bridge.yaml (the
      // default assumes the scaffold's lib/src/rust). This is the code asset
      // id RustLib.init() looks up at runtime.
      assetName: 'core/frb_generated.io.dart',
    ).run(input: input, output: output);
  });
}
