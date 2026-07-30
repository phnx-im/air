// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';

/// What kind of device the UI is rendering on. Phones run on iOS / Android,
/// everything else is treated as desktop.
///
/// Distinct from `Breakpoint`, which describes the viewport size and drives
/// layout decisions (e.g. whether the chat list and message list can sit
/// side-by-side). Device type is platform-derived and drives density.
enum DeviceType {
  phone,
  desktop;

  static DeviceType fromTargetPlatform(TargetPlatform platform) =>
      switch (platform) {
        TargetPlatform.iOS ||
        TargetPlatform.android ||
        TargetPlatform.fuchsia => phone,
        TargetPlatform.macOS ||
        TargetPlatform.windows ||
        TargetPlatform.linux => desktop,
      };

  /// Derived from [defaultTargetPlatform] rather than `dart:io` so that the
  /// device type and the typescale move together: one
  /// `debugDefaultTargetPlatformOverride` is enough for a golden to render a
  /// phone on a desktop host.
  static DeviceType get current => fromTargetPlatform(defaultTargetPlatform);

  static bool get isPhone => current == phone;
  static bool get isDesktop => current == desktop;
}
