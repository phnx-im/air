// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/core/core.dart';
import 'package:air/features/qr_scanner/qr_scanner.dart';
import 'package:flutter/material.dart';

/// A camera in portrait orientation whose frames the test sends.
class FakeScannerCamera implements ScannerCamera {
  ValueChanged<LumaFrame>? _onFrame;
  bool disposed = false;
  final focusPoints = <Offset>[];

  bool get streaming => _onFrame != null;

  void sendFrame() => _onFrame?.call(
    LumaFrame(width: 4, height: 4, bytesPerRow: 4, bytes: Uint8List(16)),
  );

  @override
  Widget buildPreview({required Widget child}) => AspectRatio(
    aspectRatio: 3 / 4,
    child: Stack(
      fit: .expand,
      children: [
        const ColoredBox(color: Colors.black),
        child,
      ],
    ),
  );

  @override
  Future<void> startFrames(ValueChanged<LumaFrame> onFrame) async =>
      _onFrame = onFrame;

  @override
  Future<void> focusOn(Offset point) async => focusPoints.add(point);

  @override
  Future<void> dispose() async {
    _onFrame = null;
    disposed = true;
  }
}
