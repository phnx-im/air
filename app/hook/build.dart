// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:convert';
import 'dart:io';

// ignore: depend_on_referenced_packages
import 'package:code_assets/code_assets.dart';
// ignore: depend_on_referenced_packages
import 'package:flutter_rust_bridge_hooks/flutter_rust_bridge_hooks.dart';

// Optional build configuration, written by `just test-flutter` and by CI
// (see .github/actions/setup-buildenv). Keys:
//
// - `skip_rust_build`: skip the Rust build, which the Flutter tests don't
//   need. See <https://github.com/dart-lang/native/issues/3237>.
// - `cargo_env`: extra environment variables for `cargo build`. The hook runs
//   with an allowlisted environment, so CI variables such as the build number
//   only reach cargo when forwarded here.
const _hookConfigPath = '.dart_tool/air_hook_config.json';

class _HookConfig {
  const _HookConfig({this.skipRustBuild = false, this.cargoEnv = const {}});

  final bool skipRustBuild;
  final Map<String, String> cargoEnv;

  static _HookConfig load(BuildInput input, BuildOutputBuilder output) {
    final file = File.fromUri(input.packageRoot.resolve(_hookConfigPath));
    if (!file.existsSync()) {
      return const _HookConfig();
    }
    output.dependencies.add(file.uri);
    final config = jsonDecode(file.readAsStringSync()) as Map<String, Object?>;
    final cargoEnv = config['cargo_env'] as Map<String, Object?>? ?? const {};
    return _HookConfig(
      skipRustBuild: config['skip_rust_build'] == true,
      cargoEnv: {
        for (final MapEntry(:key, :value) in cargoEnv.entries)
          key: value as String,
      },
    );
  }
}

// The native-assets hook protocol does not carry the Flutter build mode
// (debug/profile/release). However, the flutter tool enables link hooks only
// for AOT builds, so `linkingEnabled` is false exactly for debug builds and
// true for profile and release.
FlutterRustBridgeBuildMode _rustBuildMode({required bool linkingEnabled}) =>
    linkingEnabled
    ? FlutterRustBridgeBuildMode.release
    : FlutterRustBridgeBuildMode.debug;

void main(List<String> args) async {
  await build(args, (input, output) async {
    final config = _HookConfig.load(input, output);
    if (config.skipRustBuild) {
      return;
    }
    await FlutterRustBridgeNativeAssetsBuilder(
      buildMode: _rustBuildMode(linkingEnabled: input.config.linkingEnabled),
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
        // Must match the MinimumOSVersion Flutter writes into the framework's
        // Info.plist, otherwise App Store Connect rejects the upload.
        if (input.config.buildCodeAssets &&
            input.config.code.targetOS == OS.iOS)
          'IPHONEOS_DEPLOYMENT_TARGET':
              '${input.config.code.iOS.targetVersion}.0',
        ...config.cargoEnv,
      },
    ).run(input: input, output: output);
  });
}
