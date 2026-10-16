// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:device_preview/device_preview.dart';
import 'package:device_preview/presets.dart';
import 'package:device_preview/svg.dart';
import 'package:flutter/material.dart';

import 'content.dart';

/// [preset]'s status bar with a full battery.
SystemUiBar? fullBatteryStatusBar(DevicePreset preset) {
  final bar = preset.systemUi?.statusBar;
  final (asset, inset) = switch (preset.platform) {
    TargetPlatform.iOS => ('ios_status_bar_trailing.svg', bar?.inset),
    TargetPlatform.android => ('android_status_bar_trailing.svg', 28.0),
    _ => (null, null),
  };
  if (bar == null || asset == null) return bar;
  return SystemUiBar(
    leading: bar.leading,
    center: bar.center,
    trailing: getProjectFile('test/product_shots/assets/$asset')
        .readAsStringSync(),
    inset: inset ?? bar.inset,
    bottomInset: bar.bottomInset,
  );
}

/// Paints [bar] the way device_preview does.
class StatusBarArtwork extends StatelessWidget {
  const StatusBarArtwork({super.key, required this.bar, required this.color});

  final SystemUiBar bar;

  /// Tints the shapes that use `currentColor`.
  final Color color;

  @override
  Widget build(BuildContext context) {
    return CustomPaint(painter: _StatusBarPainter(bar, color));
  }
}

class _StatusBarPainter extends CustomPainter {
  _StatusBarPainter(this.bar, this.color);

  final SystemUiBar bar;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    _paintArtwork(canvas, size, bar.leading, -1);
    _paintArtwork(canvas, size, bar.center, 0);
    _paintArtwork(canvas, size, bar.trailing, 1);
  }

  void _paintArtwork(Canvas canvas, Size size, String source, int alignment) {
    if (source.isEmpty) return;
    final drawing = SvgDrawing.parse(source);
    final artwork = drawing.size;
    final left = switch (alignment) {
      < 0 => bar.inset,
      0 => (size.width - artwork.width) / 2,
      _ => size.width - bar.inset - artwork.width,
    };
    final bottomInset = bar.bottomInset;
    final top = bottomInset == null
        ? (size.height - artwork.height) / 2
        : size.height - bottomInset - artwork.height;
    drawing.paintInto(
      canvas,
      Offset(left, math.max(0, top)) & artwork,
      currentColor: color,
    );
  }

  @override
  bool shouldRepaint(_StatusBarPainter oldDelegate) =>
      oldDelegate.bar != bar || oldDelegate.color != color;
}
