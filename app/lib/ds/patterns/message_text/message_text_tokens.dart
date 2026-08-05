// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Geometry for a message body and the blocks a rich body is built from.
///
/// Geometry only: colors come from the message palette at paint time, and the
/// type from the typescale. A body carries prose, so we tune the values to the
/// text, not the pointer -- one set for every density.
///
/// The blocks themselves are the host's to render, which is why we publish
/// their geometry here rather than apply it: quote bars, code slabs, list
/// markers, and table rules all have to line up with the paragraphs around
/// them.
abstract final class MessageTextTokens {
  /// Space between two blocks of a body -- one paragraph and the next, a
  /// paragraph and the list under it.
  static const double blockGap = S.s8;

  /// A quoted passage: the rule down its leading edge, and the gap between
  /// that rule and the text. The quote carries no fill of its own, so the rule
  /// is the whole of what marks it.
  static const double quoteBarWidth = StrokeWidth.px2;
  static const double quoteGap = S.s8;

  /// A fenced code block, which paints its own slab inside the bubble.
  static const EdgeInsets codePadding = EdgeInsets.all(S.s12);
  static const double codeRadius = CornerRadius.px8;

  /// Space above and below a list, which needs more air than the blockGap
  /// between two paragraphs.
  static const double listPaddingY = S.s4;

  /// Space between two items of the same list.
  static const double listItemGap = S.s8;

  /// Space between a list marker and the item's text.
  static const double listMarkerGap = S.s8;

  /// Narrowest the marker column may be. A numbered list widens it to its own
  /// longest number, so the text of a single- and a double-digit item starts
  /// at the same place.
  static const double listMarkerWidth = S.s12;

  static const double bulletSize = S.s4;

  /// A task-list checkbox, which takes the bullet's place in the marker column.
  static const double checkboxSize = S.s16;
  static const double checkboxRadius = CornerRadius.px4;
  static const double checkmarkStrokeWidth = 1.75;

  static const double tableBorderWidth = StrokeWidth.px2;
  static const double tableRadius = CornerRadius.px8;
  static const EdgeInsets tableCellPadding = EdgeInsets.symmetric(
    horizontal: S.s12,
    vertical: S.s4,
  );
}
