// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

/// What kind of device the UI is rendering on. Phones run on iOS / Android,
/// everything else is treated as desktop.
///
/// Distinct from `ResponsiveScreenType`, which describes the viewport size and
/// drives layout decisions (e.g. whether the chat list and message list can sit
/// side-by-side). Device type is platform-derived and constant for the process.
enum DeviceType {
  phone,
  desktop;

  static DeviceType get current =>
      (Platform.isIOS || Platform.isAndroid) ? phone : desktop;

  static bool get isPhone => current == phone;
  static bool get isDesktop => current == desktop;
}
