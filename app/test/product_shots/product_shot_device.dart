// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

/// Supported platforms for product shot devices.
enum ProductShotPlatform { android, ios, macos, windows, linux }

/// Minimal device description used for rendering product shots.
class ProductShotDevice {
  const ProductShotDevice({
    required this.platform,
    required this.name,
    required this.screenSize,
    required this.pixelRatio,
    this.safeArea = EdgeInsets.zero,
    this.statusBarHeight,
  });

  final ProductShotPlatform platform;
  final String name;
  final Size screenSize;
  final double pixelRatio;
  final EdgeInsets safeArea;
  final double? statusBarHeight;
}

/// Predefined devices that roughly match popular configurations.
abstract final class ProductShotDevices {
  static const ProductShotDevice androidPhone = ProductShotDevice(
    platform: ProductShotPlatform.android,
    name: 'Pixel 9 Pro',
    screenSize: Size(412.0, 915.0),
    pixelRatio: 3.8,
    safeArea: EdgeInsets.only(top: 28.0),
    statusBarHeight: 36.0,
  );

  static const ProductShotDevice iosPhone = ProductShotDevice(
    platform: ProductShotPlatform.ios,
    name: 'iPhone 17',
    screenSize: Size(402.0, 874.0),
    pixelRatio: 3.0,
    safeArea: EdgeInsets.only(top: 53.0, bottom: 36.0),
    statusBarHeight: 36.0,
  );

  static const ProductShotDevice macOsWindow = ProductShotDevice(
    platform: ProductShotPlatform.macos,
    name: 'macOS Window',
    screenSize: Size(1280.0, 832.0),
    pixelRatio: 2.0,
  );

  static const ProductShotDevice windowsWindow = ProductShotDevice(
    platform: ProductShotPlatform.windows,
    name: 'Windows Window',
    screenSize: Size(1280.0, 800.0),
    pixelRatio: 1.5,
  );

  static const ProductShotDevice linuxWindow = ProductShotDevice(
    platform: ProductShotPlatform.linux,
    name: 'Linux Window',
    screenSize: Size(1280.0, 800.0),
    pixelRatio: 1.5,
  );

  static ProductShotDevice forPlatform(ProductShotPlatform platform) {
    switch (platform) {
      case ProductShotPlatform.android:
        return androidPhone;
      case ProductShotPlatform.ios:
        return iosPhone;
      case ProductShotPlatform.macos:
        return macOsWindow;
      case ProductShotPlatform.windows:
        return windowsWindow;
      case ProductShotPlatform.linux:
        return linuxWindow;
    }
  }
}
