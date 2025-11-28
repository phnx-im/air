// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:math' as math;
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

class GridOverlayInteractive extends StatefulWidget {
  const GridOverlayInteractive({
    super.key,
    this.gridSize = 16,
    this.lineColor = const Color(0x0AFF0000),
    this.lineWidth = 0.5,
    this.twoFingerTapCountToToggle = 7,
    this.rightClickCountToToggle = 7,
    this.doubleWindow = const Duration(milliseconds: 100),
    this.sequenceTimeout = const Duration(seconds: 3),
    this.tapMaxDuration = const Duration(milliseconds: 220),
    this.tapSlop = 22.0,
    this.scaleSlop = 0.06,
  });

  final double gridSize;
  final Color lineColor;
  final double lineWidth;

  final int twoFingerTapCountToToggle;
  final int rightClickCountToToggle;

  final Duration doubleWindow;
  final Duration sequenceTimeout;
  final Duration tapMaxDuration;
  final double tapSlop;
  final double scaleSlop;

  @override
  State<GridOverlayInteractive> createState() => _GridOverlayInteractiveState();
}

class _GridOverlayInteractiveState extends State<GridOverlayInteractive> {
  bool _enabled = false;

  int _twoFingerCount = 0;
  int _rightClickCount = 0;

  int? _pendingTwoFingerMs;
  int? _pendingRightClickMs;

  int _gestureStartMs = 0;
  double _maxTranslation = 0;
  double _maxScaleDev = 0;

  Timer? _seqTimer;

  void _toggle() => setState(() => _enabled = !_enabled);

  void _resetSequence() {
    _twoFingerCount = 0;
    _rightClickCount = 0;
    _pendingTwoFingerMs = null;
    _pendingRightClickMs = null;
    _seqTimer?.cancel();
    _seqTimer = null;
  }

  void _armSeqTimeout() {
    _seqTimer?.cancel();
    _seqTimer = Timer(widget.sequenceTimeout, _resetSequence);
  }

  void _incrTwoFinger(int delta) {
    _twoFingerCount += delta;
    _armSeqTimeout();
    if (_twoFingerCount >= widget.twoFingerTapCountToToggle) {
      _resetSequence();
      _toggle();
    }
  }

  void _incrRightClick(int delta) {
    _rightClickCount += delta;
    _armSeqTimeout();
    if (_rightClickCount >= widget.rightClickCountToToggle) {
      _resetSequence();
      _toggle();
    }
  }

  // --- right-clicks ---
  void _registerRightClick() {
    final now = DateTime.now().millisecondsSinceEpoch;
    final last = _pendingRightClickMs;
    if (last == null) {
      _pendingRightClickMs = now;
      return;
    }
    final within = now - last <= widget.doubleWindow.inMilliseconds;
    if (within) {
      _pendingRightClickMs = null;
      _incrRightClick(2);
    } else {
      _incrRightClick(1);
      _pendingRightClickMs = now;
    }
  }

  void _onPointerDown(PointerDownEvent e) {
    if (e.kind == PointerDeviceKind.mouse && e.buttons == kSecondaryButton) {
      _registerRightClick();
    }
  }

  // --- two-finger taps ---
  void _onScaleStart(ScaleStartDetails d) {
    if (d.pointerCount != 2) return;
    _gestureStartMs = DateTime.now().millisecondsSinceEpoch;
    _maxTranslation = 0;
    _maxScaleDev = 0;
  }

  void _onScaleUpdate(ScaleUpdateDetails d) {
    if (d.pointerCount != 2) return;
    _maxTranslation = math.max(_maxTranslation, d.focalPointDelta.distance);
    _maxScaleDev = math.max(_maxScaleDev, (d.scale - 1).abs());
  }

  void _onScaleEnd(ScaleEndDetails d) {
    if (_gestureStartMs == 0) return;
    final now = DateTime.now().millisecondsSinceEpoch;
    final durMs = now - _gestureStartMs;
    _gestureStartMs = 0;

    final isTap =
        durMs <= widget.tapMaxDuration.inMilliseconds &&
        _maxTranslation <= widget.tapSlop &&
        _maxScaleDev <= widget.scaleSlop;
    if (!isTap) return;

    final last = _pendingTwoFingerMs;
    if (last == null) {
      _pendingTwoFingerMs = now;
      return;
    }
    final within = now - last <= widget.doubleWindow.inMilliseconds;
    if (within) {
      _pendingTwoFingerMs = null;
      _incrTwoFinger(2);
    } else {
      _incrTwoFinger(1);
      _pendingTwoFingerMs = now;
    }
  }

  @override
  void dispose() {
    _seqTimer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, c) {
        final size = Size(c.maxWidth, c.maxHeight);
        return Listener(
          behavior: HitTestBehavior.translucent,
          onPointerDown: _onPointerDown, // desktop fallback
          child: GestureDetector(
            behavior: HitTestBehavior.translucent,
            onSecondaryTap: _registerRightClick,
            onScaleStart: _onScaleStart,
            onScaleUpdate: _onScaleUpdate,
            onScaleEnd: _onScaleEnd,
            child: IgnorePointer(
              ignoring: true,
              child: _enabled
                  ? CustomPaint(
                      size: size,
                      painter: _GridPainter(
                        gridSize: widget.gridSize,
                        lineColor: widget.lineColor,
                        lineWidth: widget.lineWidth,
                      ),
                    )
                  : const SizedBox.shrink(),
            ),
          ),
        );
      },
    );
  }
}

class _GridPainter extends CustomPainter {
  _GridPainter({
    required this.gridSize,
    required this.lineColor,
    required this.lineWidth,
  });
  final double gridSize;
  final Color lineColor;
  final double lineWidth;

  @override
  void paint(Canvas c, Size s) {
    final p = Paint()
      ..color = lineColor
      ..strokeWidth = lineWidth;
    for (double x = 0; x <= s.width; x += gridSize) {
      c.drawLine(Offset(x, 0), Offset(x, s.height), p);
    }
    for (double y = 0; y <= s.height; y += gridSize) {
      c.drawLine(Offset(0, y), Offset(s.width, y), p);
    }
  }

  @override
  bool shouldRepaint(covariant _GridPainter old) =>
      old.gridSize != gridSize ||
      old.lineColor != lineColor ||
      old.lineWidth != lineWidth;
}
