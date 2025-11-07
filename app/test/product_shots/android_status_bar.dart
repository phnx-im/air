// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math';

import 'package:flutter/material.dart';

class AndroidStatusBar extends StatelessWidget {
  const AndroidStatusBar({
    super.key,
    this.isLightMode = true,
    this.height = 40,
  });

  final bool isLightMode;
  final double height;

  @override
  Widget build(BuildContext context) {
    final color = isLightMode ? Colors.black : Colors.white;

    final padding = EdgeInsets.symmetric(
      horizontal: height * 0.8,
      vertical: height * 0.2,
    );
    final iconHeight = height * 0.35;
    final iconSpacing = height * 0.08;

    return SizedBox(
      width: double.infinity,
      height: height,
      child: Padding(
        padding: padding,
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            _AndroidTime(color: color, fontSize: height * 0.45),
            Row(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                SizedBox(
                  width: iconHeight,
                  height: iconHeight,
                  child: _AndroidWifi(color: color, iconHeight: iconHeight),
                ),
                SizedBox(width: iconSpacing),
                SizedBox(
                  width: iconHeight,
                  height: iconHeight,
                  child: _AndroidSignal(color: color, iconHeight: iconHeight),
                ),
                SizedBox(width: iconSpacing),
                SizedBox(
                  width: iconHeight,
                  height: iconHeight,
                  child: _AndroidBattery(
                    color: color,
                    isLightMode: isLightMode,
                    iconHeight: iconHeight,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _AndroidTime extends StatelessWidget {
  const _AndroidTime({required this.color, required this.fontSize});

  final Color color;
  final double fontSize;

  @override
  Widget build(BuildContext context) {
    return Text(
      '9:30',
      style: TextStyle(
        fontFamily: 'Roboto',
        fontSize: fontSize,
        fontWeight: FontWeight.w600,
        letterSpacing: 0.15,
        color: color,
      ),
    );
  }
}

class _AndroidSignal extends StatelessWidget {
  const _AndroidSignal({required this.color, required this.iconHeight});

  final Color color;
  final double iconHeight;

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      size: Size(iconHeight, iconHeight),
      painter: _AndroidSignalPainter(color: color),
    );
  }
}

class _AndroidSignalPainter extends CustomPainter {
  const _AndroidSignalPainter({required this.color});

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final path =
        Path()
          ..moveTo(0, size.height)
          ..lineTo(size.width, size.height)
          ..lineTo(size.width, 0)
          ..close();

    final paint =
        Paint()
          ..color = color
          ..style = PaintingStyle.fill
          ..isAntiAlias = true;

    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(covariant _AndroidSignalPainter oldDelegate) {
    return oldDelegate.color != color;
  }
}

class _AndroidWifi extends StatelessWidget {
  const _AndroidWifi({required this.color, required this.iconHeight});

  final Color color;
  final double iconHeight;

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      size: Size(iconHeight, iconHeight),
      painter: _AndroidWifiPainter(color: color),
    );
  }
}

class _AndroidWifiPainter extends CustomPainter {
  const _AndroidWifiPainter({required this.color});

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint =
        Paint()
          ..color = color
          ..style = PaintingStyle.fill
          ..isAntiAlias = true;

    final center = Offset(size.width / 2, size.height);
    final radius = min(size.width, size.height);

    final rect = Rect.fromCircle(center: center, radius: radius);
    final path =
        Path()
          ..moveTo(center.dx, center.dy)
          ..arcTo(rect, -3 * pi / 4, pi / 2, false)
          ..close();

    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(covariant _AndroidWifiPainter oldDelegate) {
    return oldDelegate.color != color;
  }
}

class _AndroidBattery extends StatelessWidget {
  const _AndroidBattery({
    required this.color,
    required this.isLightMode,
    required this.iconHeight,
  });

  final Color color;
  final bool isLightMode;
  final double iconHeight;

  @override
  Widget build(BuildContext context) {
    final capHeight = iconHeight * 0.1;
    final capWidth = iconHeight * 0.2;
    final bodyHeight = iconHeight * 0.9;
    final bodyWidth = iconHeight * 0.6;

    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Container(
          width: capWidth,
          height: capHeight,
          decoration: BoxDecoration(
            color: color,
            borderRadius: BorderRadius.circular(capHeight / 5),
          ),
        ),
        Container(
          width: bodyWidth,
          height: bodyHeight,
          decoration: BoxDecoration(
            color: color,
            borderRadius: BorderRadius.circular(bodyWidth * 0.2),
          ),
        ),
      ],
    );
  }
}
