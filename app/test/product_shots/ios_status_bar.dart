// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math';

import 'package:flutter/material.dart';

class IosStatusBar extends StatelessWidget {
  const IosStatusBar({super.key, this.color = Colors.black, this.height = 40});

  final Color color;
  final double height;

  @override
  Widget build(BuildContext context) {
    const baseHeight = 40.0;
    final padding = EdgeInsets.only(
      left: height * 48.0 / baseHeight,
      top: height * 16.0 / baseHeight,
      right: height * 28.0 / baseHeight,
    );
    final columnSpacing = height * 6.0 / baseHeight;
    final rowSpacing = height * 5.5 / baseHeight;
    final signalHeight = height * 12.0 / baseHeight;
    final wifiSize = Size(
      height * 22.0 / baseHeight,
      height * 14.0 / baseHeight,
    );
    final wifiStrokeWidth = height * 2.8 / baseHeight;
    final batterySize = Size(
      height * 25.0 / baseHeight,
      height * 14.0 / baseHeight,
    );

    return SizedBox(
      width: double.infinity,
      height: height,
      child: Padding(
        padding: padding,
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            _IosTime(color: color, fontSize: height * 18.0 / baseHeight),
            Column(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                Row(
                  spacing: rowSpacing,
                  children: [
                    _IosSignalStrength(color: color, barHeight: signalHeight),
                    _IosWifiSymbol(
                      color: color,
                      size: wifiSize,
                      strokeWidth: wifiStrokeWidth,
                    ),
                    _IosBattery(color: color, size: batterySize),
                  ],
                ),
                SizedBox(height: columnSpacing),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _IosTime extends StatelessWidget {
  const _IosTime({required this.color, required this.fontSize});

  final Color color;
  final double fontSize;

  @override
  Widget build(BuildContext context) {
    return Text(
      "9:41",
      style: TextStyle(
        fontSize: fontSize,
        fontWeight: FontWeight.bold,
        color: color,
      ),
    );
  }
}

class _IosSignalStrength extends StatelessWidget {
  const _IosSignalStrength({required this.color, required this.barHeight});

  final Color color;
  final double barHeight;

  @override
  Widget build(BuildContext context) {
    final spacing = barHeight / 6;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      spacing: spacing,
      children: [
        _IosSignalBar(color: color, height: barHeight, fraction: 0.4),
        _IosSignalBar(color: color, height: barHeight, fraction: 0.6),
        _IosSignalBar(color: color, height: barHeight, fraction: 0.8),
        _IosSignalBar(color: color, height: barHeight, fraction: 1.0),
      ],
    );
  }
}

class _IosSignalBar extends StatelessWidget {
  const _IosSignalBar({
    required this.color,
    required this.height,
    required this.fraction,
  });

  final Color color;
  final double height;
  final double fraction;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: height / 4,
      height: height * fraction,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(height / 15),
        color: color,
      ),
    );
  }
}

class _IosWifiSymbol extends StatelessWidget {
  const _IosWifiSymbol({
    required this.color,
    required this.size,
    required this.strokeWidth,
  });

  final Color color;
  final Size size;
  final double strokeWidth;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: size.width,
      height: size.height,
      child: CustomPaint(
        painter: _WifiPainter(color: color, strokeWidth: strokeWidth),
      ),
    );
  }
}

class _WifiPainter extends CustomPainter {
  const _WifiPainter({required this.color, required this.strokeWidth});

  final Color color;
  final double strokeWidth;

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height - strokeWidth / 1.5);
    const double startAngle = -pi / 2 - pi / 4;
    const double sweepAngle = pi / 2;

    final paint =
        Paint()
          ..color = color
          ..style = PaintingStyle.stroke
          ..strokeWidth = strokeWidth
          ..strokeCap = StrokeCap.square;
    final maxR = size.height;

    for (double i = 0.01; i <= .4; i += .2) {
      final r = maxR * (i / 3);
      final rect = Rect.fromCircle(center: center, radius: r);
      canvas.drawArc(rect, startAngle, sweepAngle, false, paint);
    }

    for (double i = 1; i <= 2; i++) {
      final r = maxR * (i / 3) + strokeWidth / 2;
      final rect = Rect.fromCircle(center: center, radius: r);
      canvas.drawArc(rect, startAngle, sweepAngle, false, paint);
    }
  }

  @override
  bool shouldRepaint(covariant _WifiPainter oldDelegate) {
    return oldDelegate.color != color || oldDelegate.strokeWidth != strokeWidth;
  }
}

class _IosBattery extends StatelessWidget {
  const _IosBattery({required this.color, required this.size});

  final Color color;
  final Size size;

  @override
  Widget build(BuildContext context) {
    final width = size.width;
    final height = size.height;
    final borderWidth = width / 20;
    final innerPadding = width / 22;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      spacing: width / 20,
      children: [
        Container(
          width: width,
          height: height,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(width / 6),
            border: Border.all(color: color.withAlpha(127), width: borderWidth),
          ),
          child: Padding(
            padding: EdgeInsets.all(innerPadding),
            child: Container(
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(width / 10),
                color: color,
              ),
              width: width * 0.9,
              height: height * 0.9,
            ),
          ),
        ),
        SizedBox(
          width: width / 20,
          height: height / 3.5,
          child: CustomPaint(
            painter: _BatteryCapPainter(color: color.withAlpha(127)),
          ),
        ),
      ],
    );
  }
}

class _BatteryCapPainter extends CustomPainter {
  const _BatteryCapPainter({required this.color});

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty) {
      return;
    }

    final paint =
        Paint()
          ..color = color
          ..style = PaintingStyle.fill;

    final rect = Offset.zero & size;
    final radius = Radius.circular(size.width / 1.2);
    final rrect = RRect.fromRectAndCorners(
      rect,
      topRight: radius,
      bottomRight: radius,
    );

    canvas.drawRRect(rrect, paint);
  }

  @override
  bool shouldRepaint(covariant _BatteryCapPainter oldDelegate) {
    return oldDelegate.color != color;
  }
}
