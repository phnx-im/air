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
      extraCargoEnvironmentVariables: {
        // native_toolchain_rust spawns `cargo build` with a rebuilt environment,
        // so the CI-level SQLX_OFFLINE does not reach it. Force it here so the
        // sqlx compile-time macros use the committed `coreclient/.sqlx` cache
        // instead of trying to open a live database (which has none during the
        // app build, on CI or locally).
        'SQLX_OFFLINE': '1',
        // The Rust C deps (ring, libwebp via the `cc` crate) and rustc read
        // IPHONEOS_DEPLOYMENT_TARGET to set the iOS floor. native_toolchain_rust
        // does not pass one, so it is inherited from whichever Xcode target
        // drives the hook. When that is the FlutterNativeAssets aggregate (used
        // to build the lib before the NotificationService extension links it)
        // rather than Runner, the floor is wrong and the C objects and the Rust
        // cdylib disagree, failing to link with undefined `___chkstk_darwin`.
        // Pin it to Flutter's hardcoded native-assets targetIOSVersion (13).
        // Non-iOS/macOS toolchains ignore it, so this is safe unconditionally.
        'IPHONEOS_DEPLOYMENT_TARGET': '13.0',
      },
    ).run(input: input, output: output);
  });
}
