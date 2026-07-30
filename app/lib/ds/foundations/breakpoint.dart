// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// Three-tier viewport size: `small | medium | large`.
///
/// Drives layout decisions, notably whether the chat list and the chat can sit
/// side-by-side. Distinct from `DeviceType`, which is platform-derived, constant
/// for the process, and drives density instead.
enum Breakpoint {
  small,
  medium,
  large;

  /// Upper bound of the small tier: tab bar layout below, sidebar above.
  static const double smallMaxWidth = 576;

  /// Upper bound of the medium tier.
  static const double mediumMaxWidth = 1024;

  static Breakpoint fromWidth(double width) {
    if (width < smallMaxWidth) return small;
    if (width < mediumMaxWidth) return medium;
    return large;
  }

  bool get isSmall => this == small;
}

extension BreakpointBuildContextExtension on BuildContext {
  Breakpoint get breakpoint =>
      Breakpoint.fromWidth(MediaQuery.sizeOf(this).width);
}

extension BreakpointBoxConstraintsExtension on BoxConstraints {
  Breakpoint get breakpoint => Breakpoint.fromWidth(maxWidth);
}
