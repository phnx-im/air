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
    this.device,
  });

  final Size size;
  final Color backgroundColor;
  final Color titleColor;
  final Color subtitleColor;
  final String title;
  final String subtitle;
  final ProductShotDevice? device;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final platform = device?.platform ?? _defaultPlatform();
    final dev = device ?? ProductShotDevices.forPlatform(platform);
    final frameStyle = _frameStyleFor(dev.platform);
    final statusBarHeight = _statusBarHeightFor(dev);
    final statusBar = _statusBarFor(dev.platform, statusBarHeight);
    final resolvedSafeArea = EdgeInsets.only(
      left: dev.safeArea.left,
      top: math.max(dev.safeArea.top, statusBarHeight),
      right: dev.safeArea.right,
      bottom: dev.safeArea.bottom,
    );

    return Center(
      child: Container(
        width: size.width,
        height: size.height,
        color: backgroundColor,
        child: LayoutBuilder(
          builder: (context, constraints) {
            final outerPadding = EdgeInsets.all(size.width * 0.1);
            const frameHeightFraction = 0.7;
            final frameHeight = size.height * frameHeightFraction;
            final scaleY = frameHeight / dev.screenSize.height;

            const frameWidthFraction = 0.9;
            final frameWidth = size.width * frameWidthFraction;
            final scaleX = frameWidth / dev.screenSize.width;

            final scaleFactor = math.min(scaleX, scaleY);

            final headerHeight = size.height * (1 - frameHeightFraction - 0.1);

            return Padding(
              padding: outerPadding,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  SizedBox(
                    height: headerHeight,
                    child: Center(
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.start,
                        crossAxisAlignment: CrossAxisAlignment.center,
                        children: [
                          SizedBox(height: headerHeight * 0.1),
                          _ShotTitle(
                            text: title,
                            color: titleColor,
                            size: size.width,
                          ),
                          SizedBox(height: headerHeight * 0.05),
                          _ShotSubtitle(
                            text: subtitle,
                            color: subtitleColor,
                            size: size.width,
                          ),
                        ],
                      ),
                    ),
                  ),
                  Align(
                    alignment: Alignment.topCenter,
                    child: Transform.scale(
                      scale: scaleFactor,
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
      fontWeight: FontWeight.w800,
      height: 1.5,
      letterSpacing: -fontSize / 128,
      color: color,
    );
    return DefaultTextStyle.merge(
      style: textStyle,
      child: Text(
        text,
        maxLines: 2,
        textAlign: TextAlign.center,
        overflow: TextOverflow.ellipsis,
      ),
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
      fontWeight: FontWeight.w500,
      height: 1.5,
      letterSpacing: -fontSize / 128,
      color: color,
    );
    return DefaultTextStyle.merge(
      style: textStyle,
      child: Text(
        text,
        maxLines: 2,
        textAlign: TextAlign.center,
        overflow: TextOverflow.ellipsis,
      ),
    );
  }
}

ProductShotPlatform _defaultPlatform() {
  return Platform.isAndroid
      ? ProductShotPlatform.android
      : ProductShotPlatform.ios;
}

_FrameStyle _frameStyleFor(ProductShotPlatform platform) {
  switch (platform) {
    case ProductShotPlatform.android:
      return const _FrameStyle(
        borderWidth: 20,
        cornerRadius: 48,
        frameColor: Color(0xFFF2F4F6),
        frameHeightFraction: 0.94,
        verticalOffsetFraction: 0.12,
      );
    case ProductShotPlatform.ios:
      return const _FrameStyle(
        borderWidth: 18,
        cornerRadius: 64,
        frameColor: Color(0xFFF2F4F6),
        frameHeightFraction: 0.94,
        verticalOffsetFraction: 0.12,
      );
    case ProductShotPlatform.macos:
      return const _FrameStyle(
        borderWidth: 28,
        cornerRadius: 48,
        frameColor: Color(0xFFF2F4F6),
        frameHeightFraction: 0.82,
        verticalOffsetFraction: 0.12,
      );
    case ProductShotPlatform.windows:
    case ProductShotPlatform.linux:
      return const _FrameStyle(
        frameHeightFraction: 0.82,
        verticalOffsetFraction: 0.12,
      );
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
    case ProductShotPlatform.android:
      return 40.0;
    case ProductShotPlatform.ios:
      return 44.0;
    case ProductShotPlatform.macos:
    case ProductShotPlatform.windows:
    case ProductShotPlatform.linux:
      return 0.0;
  }
}

Widget _statusBarFor(ProductShotPlatform platform, double statusBarHeight) {
  switch (platform) {
    case ProductShotPlatform.android:
      return AndroidStatusBar(height: statusBarHeight);
    case ProductShotPlatform.ios:
      return IosStatusBar(height: statusBarHeight);
    case ProductShotPlatform.macos:
    case ProductShotPlatform.windows:
    case ProductShotPlatform.linux:
      return const SizedBox.shrink();
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
