// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/dimensions.dart';
import 'package:flutter/foundation.dart';

/// Geometry every surface's chrome shares: the bar across its top, and the
/// controls that sit in or beside it.
///
/// These are here rather than in the patterns that use them because the values
/// only mean anything when they agree. A bar that is 56 tall on one screen and
/// 52 on the next has no property worth naming.
abstract final class Chrome {
  /// Height of every bar in the app: list header, chat header, modal header,
  /// the fullscreen viewer's strip. One height means a button centres at the
  /// same y wherever it appears, so pushing between two surfaces never shifts
  /// the chrome.
  static const double barHeight = S.s56;

  /// Inset from a surface's leading edge to the first control in it. The
  /// header's back button and the composer's attach button both take it, so
  /// they land on one x down the screen instead of stepping inward.
  static const double edgeInset = S.s20;

  /// Gap between a round control and what sits beside it: the composer's
  /// buttons and its field, the message row's avatar and its bubble.
  static const double controlGap = S.s8;

  /// Inset from the leading edge to the first control on a surface reaching
  /// the window's top-left corner, where macOS floats the traffic lights. The
  /// width is the rail's, which is what they float over in the main window.
  static const double windowControlsInset = S.s80;

  /// Whether the native window controls float over the content at the window's
  /// top-left. Only macOS does: Windows puts its controls on the far right and
  /// Linux draws a header bar of its own.
  ///
  /// We read [defaultTargetPlatform] rather than `dart:io`, so the layout
  /// follows the platform a test pins rather than the host it runs on.
  static bool get windowControlsFloat =>
      defaultTargetPlatform == TargetPlatform.macOS;
}
