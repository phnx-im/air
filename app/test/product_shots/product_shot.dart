// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'android_status_bar.dart';
import 'ios_status_bar.dart';
import 'product_shot_frame.dart';
import 'product_shot_device.dart';

class ProductShot extends StatelessWidget {
  const ProductShot({
    super.key,
    required this.size,
    required this.backgroundColor,
    required this.titleColor,
    required this.subtitleColor,
    required this.title,
    required this.subtitle,
    required this.child,
    required this.frameColor,
    required this.device,
  });

  /// The marketing canvas size, distinct from [ProductShotDevice.screenSize]
  /// (the device frame drawn inside it, scaled to fit).
  final Size size;
  final Color backgroundColor;
  final Color titleColor;
  final Color subtitleColor;
  final String title;
  final String subtitle;
  final ProductShotDevice device;
  final Color frameColor;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final dev = device;
    final frameStyle = _frameStyleFor(dev.platform, frameColor);
    final statusBarHeight = _statusBarHeightFor(dev);
    final statusBar = _statusBarFor(dev.platform, statusBarHeight);
    final resolvedSafeArea = EdgeInsets.only(
      left: dev.safeArea.left,
      top: math.max(dev.safeArea.top, statusBarHeight),
      right: dev.safeArea.right,
      bottom: dev.safeArea.bottom,
    );

    // Landscape canvases (desktop store screenshots) get their own spacing:
    // the portrait fractions are tuned for phone shots and would overflow.
    final isLandscape = size.width > size.height;

    return Center(
      child: Container(
        width: size.width,
        height: size.height,
        color: backgroundColor,
        child: LayoutBuilder(
          builder: (context, constraints) {
            final outerPadding = EdgeInsets.all(
              isLandscape ? size.height * 0.05 : size.width * 0.1,
            );
            // Fractions are budgeted against the space left after padding,
            // not the raw canvas, so header + frame always fit regardless of
            // the canvas aspect ratio.
            final availableWidth = size.width - outerPadding.horizontal;
            final availableHeight = size.height - outerPadding.vertical;

            final frameHeightFraction = isLandscape ? 0.62 : 0.7;
            final frameHeight = availableHeight * frameHeightFraction;

            const frameWidthFraction = 0.9;
            final frameWidth = availableWidth * frameWidthFraction;

            final headerHeight = isLandscape
                ? availableHeight * 0.24
                : availableHeight * (1 - frameHeightFraction - 0.1);
            // Font sizes derive from the canvas width, which is far too large
            // on a landscape canvas, so reference the height there instead.
            final fontReference = isLandscape ? size.height : size.width;

            return Padding(
              padding: outerPadding,
              child: Column(
                crossAxisAlignment: .center,
                children: [
                  SizedBox(
                    height: headerHeight,
                    child: Align(
                      alignment: Alignment.topCenter,
                      child: FittedBox(
                        fit: .scaleDown,
                        child: Column(
                          mainAxisAlignment: .start,
                          crossAxisAlignment: .center,
                          children: [
                            SizedBox(height: headerHeight * 0.05),
                            _ShotTitle(
                              text: title,
                              color: titleColor,
                              size: fontReference,
                            ),
                            SizedBox(height: headerHeight * 0.05),
                            _ShotSubtitle(
                              text: subtitle,
                              color: subtitleColor,
                              size: fontReference,
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  Align(
                    alignment: Alignment.topCenter,
                    child: SizedBox(
                      width: frameWidth,
                      height: frameHeight,
                      child: FittedBox(
                        fit: .contain,
                        alignment: Alignment.topCenter,
                        child: ProductShotFrame(
                          statusBar: statusBar,
                          statusBarHeight: statusBarHeight,
                          screenSize: dev.screenSize,
                          devicePixelRatio: dev.pixelRatio,
                          safeArea: resolvedSafeArea,
                          borderWidth: frameStyle.borderWidth,
                          cornerRadius: frameStyle.cornerRadius,
                          frameColor: frameStyle.frameColor,
                          child: child,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

/// Large store headline used in product shots.
class _ShotTitle extends StatelessWidget {
  const _ShotTitle({
    required this.text,
    required this.color,
    required this.size,
  });

  final String text;
  final Color color;
  final double size;

  @override
  Widget build(BuildContext context) {
    final fontSize = size / 16;
    final textStyle = TextStyle(
      fontSize: fontSize,
      fontWeight: .w800,
      height: 1.3,
      letterSpacing: -fontSize / 128,
      color: color,
    );
    return DefaultTextStyle.merge(
      style: textStyle,
      child: Text(text, maxLines: 2, textAlign: .center, overflow: .ellipsis),
    );
  }
}

/// Large store headline used in product shots.
class _ShotSubtitle extends StatelessWidget {
  const _ShotSubtitle({
    required this.text,
    required this.color,
    required this.size,
  });

  final String text;
  final Color color;
  final double size;

  @override
  Widget build(BuildContext context) {
    final fontSize = size / 24;
    final textStyle = TextStyle(
      fontSize: fontSize,
      fontWeight: .w500,
      height: 1.3,
      letterSpacing: -fontSize / 128,
      color: color,
    );
    return DefaultTextStyle.merge(
      style: textStyle,
      child: Text(text, maxLines: 2, textAlign: .center, overflow: .ellipsis),
    );
  }
}

_FrameStyle _frameStyleFor(TargetPlatform platform, Color frameColor) {
  switch (platform) {
    case TargetPlatform.android:
      return _FrameStyle(
        borderWidth: 20,
        cornerRadius: 48,
        frameColor: frameColor,
        frameHeightFraction: 0.94,
        verticalOffsetFraction: 0.12,
      );
    case TargetPlatform.iOS:
      return _FrameStyle(
        borderWidth: 18,
        cornerRadius: 64,
        frameColor: frameColor,
        frameHeightFraction: 0.94,
        verticalOffsetFraction: 0.12,
      );
    case TargetPlatform.macOS:
      return _FrameStyle(
        borderWidth: 28,
        cornerRadius: 48,
        frameColor: frameColor,
        frameHeightFraction: 0.82,
        verticalOffsetFraction: 0.12,
      );
    case TargetPlatform.windows:
    case TargetPlatform.linux:
      return _FrameStyle(
        frameColor: frameColor,
        frameHeightFraction: 0.82,
        verticalOffsetFraction: 0.12,
      );
    default:
      throw "Unsupported platform";
  }
}

double _statusBarHeightFor(ProductShotDevice device) {
  if (device.statusBarHeight != null) {
    return device.statusBarHeight!;
  }
  if (device.safeArea.top > 0) {
    return device.safeArea.top;
  }

  switch (device.platform) {
    case TargetPlatform.android:
      return 40.0;
    case TargetPlatform.iOS:
      return 44.0;
    case TargetPlatform.macOS:
    case TargetPlatform.windows:
    case TargetPlatform.linux:
      return 0.0;
    default:
      throw "Unsupported platform";
  }
}

Widget _statusBarFor(TargetPlatform platform, double statusBarHeight) {
  switch (platform) {
    case TargetPlatform.android:
      return AndroidStatusBar(height: statusBarHeight);
    case TargetPlatform.iOS:
      return IosStatusBar(height: statusBarHeight);
    case TargetPlatform.macOS:
    case TargetPlatform.windows:
    case TargetPlatform.linux:
      return const SizedBox.shrink();
    default:
      throw "Unsupported platform";
  }
}

class _FrameStyle {
  const _FrameStyle({
    this.borderWidth = 32,
    this.cornerRadius = 80,
    this.frameColor = Colors.black,
    this.frameHeightFraction = 0.80,
    this.verticalOffsetFraction = 0.20,
  });

  final double borderWidth;
  final double cornerRadius;
  final Color frameColor;
  final double frameHeightFraction;
  final double verticalOffsetFraction;
}
