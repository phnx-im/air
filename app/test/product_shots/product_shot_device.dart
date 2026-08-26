// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:device_frame/device_frame.dart';
import 'package:flutter/material.dart';

extension ProductShotPlatformExt on TargetPlatform {
  String get identifier {
    switch (this) {
      case TargetPlatform.android:
        return "android";
      case TargetPlatform.iOS:
        return "ios";
      case TargetPlatform.linux:
        return "linux";
      case TargetPlatform.macOS:
        return "macos";
      case TargetPlatform.windows:
        return "windows";
      default:
        throw "Unsupported platform";
    }
  }

  ProductShotDevice get device {
    switch (this) {
      case TargetPlatform.android:
        return ProductShotDevice(
          platform: TargetPlatform.android,
          name: 'Pixel 9 Pro',
          // screenSize: Size(412.0, 915.0),
          deviceInfo: Devices.android.googlePixel9,
          pixelRatio: 3.8,
          safeArea: const EdgeInsets.only(top: 28.0),
          statusBarHeight: 36.0,
        );
      case TargetPlatform.iOS:
        return ProductShotDevice(
          platform: TargetPlatform.iOS,
          name: 'iPhone 17',
          // screenSize: Size(402.0, 874.0),
          deviceInfo: Devices.ios.iPhone16Pro,
          pixelRatio: 3.0,
          safeArea: const EdgeInsets.only(top: 53.0, bottom: 36.0),
          statusBarHeight: 36.0,
        );
      case TargetPlatform.macOS:
        return ProductShotDevice(
          platform: TargetPlatform.macOS,
          name: 'macOS Laptop',
          // screenSize: Size(1280.0, 832.0),
          deviceInfo: Devices.macOS.macBookPro,
          pixelRatio: 2.0,
        );
      case TargetPlatform.windows:
        return ProductShotDevice(
          platform: TargetPlatform.windows,
          name: 'Windows Laptop',
          // screenSize: Size(1280.0, 800.0),
          deviceInfo: Devices.windows.laptop,
          pixelRatio: 1.5,
        );
      case TargetPlatform.linux:
        return ProductShotDevice(
          platform: TargetPlatform.linux,
          name: 'Linux Laptop',
          // screenSize: Size(1280.0, 800.0),
          deviceInfo: Devices.linux.laptop,
          pixelRatio: 1.5,
        );
      default:
        throw "Unsupported target platform";
    }
  }
}

/// Minimal device description used for rendering product shots.
class ProductShotDevice {
  const ProductShotDevice({
    required this.platform,
    required this.name,
    required this.deviceInfo,
    // required this.screenSize,
    required this.pixelRatio,
    this.safeArea = EdgeInsets.zero,
    this.statusBarHeight,
  });

  final TargetPlatform platform;
  final DeviceInfo deviceInfo;
  final String name;
  // final Size screenSize;
  final double pixelRatio;
  final EdgeInsets safeArea;
  final double? statusBarHeight;

  String get identifier => deviceInfo.identifier.name;

  Size get screenSize => deviceInfo.frameSize;
}
