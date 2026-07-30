// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/foundation.dart';

/// What kind of device the UI is rendering on. Phones run on iOS / Android,
/// everything else is treated as desktop.
///
/// Distinct from `Breakpoint`, which describes the viewport size and drives
/// layout decisions (e.g. whether the chat list and message list can sit
/// side-by-side). Device type is platform-derived and constant for the process.
enum DeviceType {
  phone,
  desktop;

  /// Pins [current] regardless of the host platform. Golden tests render phone
  /// viewports on a desktop host, so they set this to keep the two in step.
  @visibleForTesting
  static DeviceType? debugOverride;

  static DeviceType get current =>
      debugOverride ??
      ((Platform.isIOS || Platform.isAndroid) ? phone : desktop);

  static bool get isPhone => current == phone;
  static bool get isDesktop => current == desktop;
}
