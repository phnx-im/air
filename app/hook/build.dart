// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:convert';
import 'dart:io';

// ignore: depend_on_referenced_packages
import 'package:code_assets/code_assets.dart';
// ignore: depend_on_referenced_packages
import 'package:flutter_rust_bridge_hooks/flutter_rust_bridge_hooks.dart';

// Written by `just test-flutter` to skip rust build.
// See <https://github.com/dart-lang/native/issues/3237>
const _hookConfigPath = '.dart_tool/air_hook_config.json';

bool _skipRustBuild(BuildInput input, BuildOutputBuilder output) {
  final file = File.fromUri(input.packageRoot.resolve(_hookConfigPath));
  if (!file.existsSync()) {
    return false;
  }
  output.dependencies.add(file.uri);
  final config = jsonDecode(file.readAsStringSync()) as Map<String, Object?>;
  return config['skip_rust_build'] == true;
}

// The native-assets hook protocol does not carry the Flutter build mode
// (debug/profile/release). However, the flutter tool enables link hooks only
// for AOT builds, so `linkingEnabled` is false exactly for debug builds and
// true for profile and release.
FlutterRustBridgeBuildMode _rustBuildMode({required bool linkingEnabled}) =>
    linkingEnabled
    ? FlutterRustBridgeBuildMode.release
    : FlutterRustBridgeBuildMode.debug;

// Android runs 64-bit devices with 16 KB memory pages, which requires shared
// libraries aligned to the same size.
// See <https://developer.android.com/guide/practices/page-sizes>
const _pageSizeRustFlags =
    '-C link-arg=-Wl,-z,max-page-size=16384 '
    '-C link-arg=-Wl,-z,common-page-size=16384';

const _pageSizeAlignedArchitectures = [Architecture.arm64, Architecture.x64];

bool _needsPageSizeRustFlags(BuildInput input) {
  final config = input.config;
  return config.buildCodeAssets &&
      config.code.targetOS == OS.android &&
      _pageSizeAlignedArchitectures.contains(config.code.targetArchitecture);
}

// Extend the ambient RUSTFLAGS with the page size flags
String _pageSizeRustFlagsWithAmbient() {
  final ambient = Platform.environment['RUSTFLAGS'] ?? '';
  return ambient.isEmpty ? _pageSizeRustFlags : '$ambient $_pageSizeRustFlags';
}

void main(List<String> args) async {
  await build(args, (input, output) async {
    if (_skipRustBuild(input, output)) {
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
        if (_needsPageSizeRustFlags(input))
          'RUSTFLAGS': _pageSizeRustFlagsWithAmbient(),
        // Must match the MinimumOSVersion Flutter writes into the framework's
        // Info.plist, otherwise App Store Connect rejects the upload.
        if (input.config.buildCodeAssets &&
            input.config.code.targetOS == OS.iOS)
          'IPHONEOS_DEPLOYMENT_TARGET':
              '${input.config.code.iOS.targetVersion}.0',
      },
    ).run(input: input, output: output);
  });
}
