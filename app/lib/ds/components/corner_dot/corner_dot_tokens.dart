// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';

/// Layout tokens for the corner dot.
///
/// Geometry only: the fill comes from the palette at paint time. One set: the
/// dot marks a state rather than carrying content, so it reads the same on a
/// thumb-sized button and a pointer-sized one.
abstract final class CornerDotTokens {
  static const double size = S.s8;

  /// The dot sits flush with the right edge, where both the back arrow and the
  /// scroll-back chevron leave the most room.
  static const double insetTop = S.s2;
  static const double insetRight = S.s0;
}
