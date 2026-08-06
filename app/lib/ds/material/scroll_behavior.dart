// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

/// Scroll behavior that matches Flutter's base [ScrollBehavior] physics:
/// bouncing on Apple platforms, clamping elsewhere.
///
/// [MaterialScrollBehavior] inherits [ScrollBehavior.getScrollPhysics] which
/// already does this, but an explicit override ensures the correct behavior
/// regardless of future Material changes and makes the intent visible.
class AppScrollBehavior extends MaterialScrollBehavior {
  const AppScrollBehavior();

  // iOS: bouncing with normal deceleration (touch flicks).
  static const _bouncingPhysics = BouncingScrollPhysics(
    parent: RangeMaintainingScrollPhysics(),
  );

  // macOS: bouncing with fast deceleration (trackpad flicks stop sooner).
  static const _bouncingDesktopPhysics = BouncingScrollPhysics(
    decelerationRate: ScrollDecelerationRate.fast,
    parent: RangeMaintainingScrollPhysics(),
  );

  static const _clampingPhysics = ClampingScrollPhysics(
    parent: RangeMaintainingScrollPhysics(),
  );

  @override
  ScrollPhysics getScrollPhysics(BuildContext context) {
    return switch (getPlatform(context)) {
      TargetPlatform.iOS => _bouncingPhysics,
      TargetPlatform.macOS => _bouncingDesktopPhysics,
      _ => _clampingPhysics,
    };
  }
}
