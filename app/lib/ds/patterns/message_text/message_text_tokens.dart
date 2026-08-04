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
@immutable
class MessageTextTokens {
  const MessageTextTokens({
    required this.blockGap,
    required this.quoteBarWidth,
    required this.quoteGap,
    required this.codePadding,
    required this.codeRadius,
    required this.listPaddingY,
    required this.listItemGap,
    required this.listMarkerGap,
    required this.listMarkerWidth,
    required this.bulletSize,
    required this.checkboxSize,
    required this.checkboxRadius,
    required this.checkmarkStrokeWidth,
    required this.tableBorderWidth,
    required this.tableRadius,
    required this.tableCellPadding,
  });

  /// Space between two blocks of a body -- one paragraph and the next, a
  /// paragraph and the list under it.
  final double blockGap;

  /// A quoted passage: the rule down its leading edge, and the gap between
  /// that rule and the text. The quote carries no fill of its own, so the rule
  /// is the whole of what marks it.
  final double quoteBarWidth;
  final double quoteGap;

  /// A fenced code block, which paints its own slab inside the bubble.
  final EdgeInsets codePadding;
  final double codeRadius;

  /// Space above and below a list, which needs more air than the blockGap
  /// between two paragraphs.
  final double listPaddingY;

  /// Space between two items of the same list.
  final double listItemGap;

  /// Space between a list marker and the item's text.
  final double listMarkerGap;

  /// Narrowest the marker column may be. A numbered list widens it to its own
  /// longest number, so the text of a single- and a double-digit item starts
  /// at the same place.
  final double listMarkerWidth;

  final double bulletSize;

  /// A task-list checkbox, which takes the bullet's place in the marker column.
  final double checkboxSize;
  final double checkboxRadius;
  final double checkmarkStrokeWidth;

  final double tableBorderWidth;
  final double tableRadius;
  final EdgeInsets tableCellPadding;

  static const MessageTextTokens standard = MessageTextTokens(
    blockGap: S.s8,
    quoteBarWidth: StrokeWidth.px2,
    quoteGap: S.s8,
    codePadding: EdgeInsets.all(S.s12),
    codeRadius: CornerRadius.px8,
    listPaddingY: S.s4,
    listItemGap: S.s8,
    listMarkerGap: S.s8,
    listMarkerWidth: S.s12,
    bulletSize: S.s4,
    checkboxSize: S.s16,
    checkboxRadius: CornerRadius.px4,
    checkmarkStrokeWidth: 1.75,
    tableBorderWidth: StrokeWidth.px2,
    tableRadius: CornerRadius.px8,
    tableCellPadding: EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s4),
  );
}
