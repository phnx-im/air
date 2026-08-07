// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Fade values for the profile tab's scroll frame, per density.
///
/// The phone screen is full-bleed and floats its chrome over the content, so
/// each strip runs deeper than the bar it beds. The pane sits inside a panel
/// where the bar is the only thing to cover, so each strip is just the bar.
@immutable
class YouFadeTokens {
  const YouFadeTokens({required this.topHeight, required this.bottomHeight});

  final double topHeight;
  final double bottomHeight;

  /// Fraction of each strip held at full strength before its ramp starts.
  /// Gradient positions rather than alphas, so they stay literal.
  static const double topSolidStop = 0.3;
  static const double bottomSolidStop = 0.1;

  /// Both strips reach full strength at their edge: what slides under them is
  /// chrome, which has to occlude rather than tint.
  static const double bottomOpacity = Alpha.a100;

  static const YouFadeTokens phone = YouFadeTokens(
    topHeight: S.s96,
    bottomHeight: S.s120,
  );

  static const YouFadeTokens desktop = YouFadeTokens(
    topHeight: Chrome.barHeight,
    bottomHeight: Chrome.barHeight,
  );
}
