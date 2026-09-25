// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The device-shell product shot: the app inside real device artwork, with no
/// marketing chrome around it.
///
/// Distinct from [ProductShot], which frames the app in a tinted bezel of its
/// own on a marketing canvas. That one is what this repo records; these are
/// recorded into the private campaign directories, which supply the artwork.
library;

import 'dart:io';

import 'package:device_preview/device_preview.dart';
import 'package:device_preview/presets.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:path/path.dart' as p;
import 'package:yaml/yaml.dart';

/// Override with `--dart-define=DEVICE_SPECS_DIR=path/to/dir`. May be absolute
/// (e.g. for the artwork that lives outside this repo checkout).
const _deviceSpecsDir = String.fromEnvironment('DEVICE_SPECS_DIR');

/// The spec slot and the packaged preset it patches, per platform.
(String, DevicePreset) _slotFor(TargetPlatform platform) => switch (platform) {
  TargetPlatform.android => ('google-pixel-9', DevicePresets.pixel9),
  TargetPlatform.iOS => ('apple-iphone-17', DevicePresets.iPhone17),
  TargetPlatform.macOS => ('macbook-pro', DevicePresets.largeDesktopWindow),
  TargetPlatform.windows => (
    'windows-desktop',
    DevicePresets.largeDesktopWindow,
  ),
  TargetPlatform.linux => ('linux-desktop', DevicePresets.largeDesktopWindow),
  _ => throw "Unsupported target platform",
};

extension DeviceShotPlatformExt on TargetPlatform {
  DeviceShotDevice deviceShot({Brightness brightness = Brightness.light}) {
    final (slot, base) = _slotFor(this);
    return DeviceShotDevice(
      platform: this,
      preset: _resolvePreset(slot, base, this, brightness),
      brightness: brightness,
    );
  }
}

/// The device a shot depicts, resolved from a [DevicePreset].
class DeviceShotDevice {
  DeviceShotDevice({
    required this.platform,
    required this.preset,
    Brightness brightness = Brightness.light,
  }) : simulation = preset.resolve().copyWith(platformBrightness: brightness);

  /// The depicted target platform.
  final TargetPlatform platform;

  /// Device parameters definition.
  final DevicePreset preset;
  final DeviceSimulation simulation;

  /// Full device name.
  String get name => preset.name;

  /// Normalized device ID (useful for filenames for example).
  String get identifier => preset.id;

  /// Output pixels per logical pixel for this device's shot.
  double get exportScale => simulation.devicePixelRatio ?? 1.0;

  /// The logical content area the embedded app renders into.
  Size get screenSize => simulation.screenSize!;

  /// The body artwork's footprint, in screen coordinates.
  Rect get frameBounds =>
      simulation.frame?.bodyBounds(screenSize, simulation.orientation) ??
      (Offset.zero & screenSize);
}

/// Draws [child] inside the device's real frame artwork, as one widget among
/// others.
class DeviceShotFrame extends StatelessWidget {
  const DeviceShotFrame({
    super.key,
    required this.device,
    required this.child,
    this.scale = 1.0,
  });

  final DeviceShotDevice device;
  final Widget child;

  /// How many output pixels to emit per logical pixel of the device.
  final double scale;

  @override
  Widget build(BuildContext context) {
    final simulation = device.simulation;
    final screenSize = device.screenSize;
    final bounds = device.frameBounds;
    final baseMediaQuery =
        MediaQuery.maybeOf(context) ?? const MediaQueryData();

    final screen = MediaQuery(
      data: baseMediaQuery.copyWith(
        size: screenSize,
        devicePixelRatio: simulation.devicePixelRatio,
        padding: simulation.padding,
        viewPadding: simulation.viewPadding,
        viewInsets: EdgeInsets.zero,
        systemGestureInsets: simulation.systemGestureInsets,
      ),
      child: child,
    );

    // The frame is laid out at exactly the screen size and paints its body
    // *outside* those bounds, so reserve the body's footprint and offset the
    // screen to where the artwork expects it.
    final framed = SizedBox.fromSize(
      size: bounds.size,
      child: Stack(
        clipBehavior: .none,
        children: [
          Positioned(
            left: -bounds.left,
            top: -bounds.top,
            width: screenSize.width,
            height: screenSize.height,
            child: DevicePreviewFrame(
              simulation: _FixedSimulation(simulation),
              child: screen,
            ),
          ),
        ],
      ),
    );

    return RepaintBoundary(
      child: SizedBox.fromSize(
        size: bounds.size * scale,
        // Same aspect ratio either side, so `contain` scales by exactly
        // [scale] without letterboxing.
        child: FittedBox(fit: .contain, child: framed),
      ),
    );
  }
}

class _FixedSimulation implements ValueListenable<DeviceSimulation?> {
  const _FixedSimulation(this.value);

  @override
  final DeviceSimulation? value;

  @override
  void addListener(VoidCallback listener) {}

  @override
  void removeListener(VoidCallback listener) {}
}

/// Resolved presets, keyed by slot and brightness.
final _presets = <String, DevicePreset>{};

/// [base] deep-patched with `<slot>.yaml` from [_deviceSpecsDir], and then
/// for a dark theme shot with that spec's own `dark:` section.
DevicePreset _resolvePreset(
  String slot,
  DevicePreset base,
  TargetPlatform platform,
  Brightness brightness,
) {
  final isDark = brightness == Brightness.dark;
  return _presets.putIfAbsent('$slot${isDark ? '.dark' : ''}', () {
    var json = base.toJson();
    final spec = _loadSpec(slot);
    if (spec != null) {
      json = _deepPatch(json, spec);
      final dark = json.remove('dark');
      if (isDark && dark is Map<String, Object?>) {
        json = _deepPatch(json, dark);
      }
    }
    json['id'] = slot;
    json['platform'] = platform.name;
    return DevicePreset.fromJson(json);
  });
}

/// The spec for [slot], or null when no override directory is configured or it
/// carries no spec for this device.
Map<String, Object?>? _loadSpec(String slot) {
  if (_deviceSpecsDir.isEmpty) return null;
  final file = File(p.join(_deviceSpecsDir, '$slot.yaml'));
  if (!file.existsSync()) return null;

  final spec = _plain(loadYaml(file.readAsStringSync()));
  if (spec is! Map<String, Object?>) {
    throw FormatException('${file.path}: expected a YAML mapping');
  }
  return spec;
}

/// [base] with [patch] applied, recursing into nested mappings so a patch can
/// name a single leaf.
Map<String, Object?> _deepPatch(
  Map<String, Object?> base,
  Map<String, Object?> patch,
) {
  final result = Map<String, Object?>.of(base);
  patch.forEach((key, value) {
    final existing = result[key];
    result[key] =
        existing is Map<String, Object?> && value is Map<String, Object?>
        ? _deepPatch(existing, value)
        : value;
  });
  return result;
}

/// Rebuilds a `loadYaml` tree out of plain maps and lists, which is what
/// [DevicePreset.fromJson] expects.
Object? _plain(Object? node) => switch (node) {
  final Map<Object?, Object?> map => <String, Object?>{
    for (final entry in map.entries) '${entry.key}': _plain(entry.value),
  },
  final Iterable<Object?> list => list.map(_plain).toList(),
  _ => node,
};
