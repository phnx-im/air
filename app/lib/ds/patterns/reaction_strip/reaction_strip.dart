// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/components/reaction_chip/reaction_chip.dart';
import 'package:air/ds/components/reaction_chip/reaction_chip_tokens.dart';
import 'package:air/ds/patterns/reaction_strip/reaction_strip_tokens.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// One distinct emoji on a message and how many people reacted with it.
@immutable
class ReactionGroup {
  const ReactionGroup({
    required this.emoji,
    required this.count,
    this.mine = false,
  });

  final String emoji;

  /// Number of people who reacted with this emoji.
  final int count;

  /// Whether the current user is one of them.
  final bool mine;
}

/// The run of reaction chips shown beneath a message.
///
/// We order chips most- to least-reacted and pack them into the width the
/// message gives the strip. Once the next chip would exceed that width the rest
/// collapse into a trailing `+N` chip, so a narrow bubble keeps the reactions
/// that matter instead of clipping them. Anchored to the leading edge, inset
/// from the bubble's start, and lifted to overlap it.
class ReactionStrip extends StatelessWidget {
  const ReactionStrip({
    super.key,
    required this.tokens,
    required this.chipTokens,
    required this.groups,
    this.onTapEmoji,
    this.onTapOverflow,
  });

  final ReactionStripTokens tokens;
  final ReactionChipTokens chipTokens;
  final List<ReactionGroup> groups;

  /// Tapping a chip, to add or take back that reaction.
  final void Function(String emoji)? onTapEmoji;

  /// Tapping the trailing `+N`, which stands in for the collapsed reactions
  /// and so reveals all of them rather than toggling one.
  final VoidCallback? onTapOverflow;

  @override
  Widget build(BuildContext context) {
    if (groups.isEmpty) return const SizedBox.shrink();

    final ordered = _mostReactedFirst();
    final metrics = _ChipMetrics.measure(context, chipTokens, ordered);

    return _StripFootprint(
      minimalWidth: ReactionStripTokens.startInset + metrics.minimalWidth,
      runWidth: ReactionStripTokens.startInset + _runWidth(metrics),
      height: metrics.height,
      child: Align(
        // Overrides the message column's end-alignment for own messages, so the
        // run starts at the bubble's leading edge either way.
        alignment: Alignment.centerLeft,
        child: Transform.translate(
          // Lift so the pill, not the chip box, rides `overlap` up into the
          // bubble: the crop ring sits outside the pill, so add it back in.
          offset: Offset(0, -tokens.lift),
          child: Padding(
            padding: const EdgeInsets.only(
              left: ReactionStripTokens.startInset,
            ),
            child: LayoutBuilder(
              builder: (context, constraints) => SizedBox(
                height: metrics.height,
                child: _row(
                  _pack(constraints.maxWidth, ordered, metrics),
                  bounded: constraints.hasBoundedWidth,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// Most-reacted first, ties keeping the order they arrived in.
  List<ReactionGroup> _mostReactedFirst() {
    final indexed =
        [for (var i = 0; i < groups.length; i++) (group: groups[i], index: i)]
          ..sort((a, b) {
            final byCount = b.group.count.compareTo(a.group.count);
            return byCount != 0 ? byCount : a.index.compareTo(b.index);
          });
    return [for (final entry in indexed) entry.group];
  }

  /// What the run takes with every chip shown.
  double _runWidth(_ChipMetrics metrics) {
    var total = 0.0;
    for (var i = 0; i < metrics.widths.length; i++) {
      total += metrics.widths[i] + (i > 0 ? tokens.spacing : 0);
    }
    return total;
  }

  /// The chips that fit [maxWidth], plus a trailing `+N` for the rest.
  List<Widget> _pack(
    double maxWidth,
    List<ReactionGroup> ordered,
    _ChipMetrics metrics,
  ) {
    final count = ordered.length;
    final limit = maxWidth - ReactionStripTokens.fitSlack;
    final full = _runWidth(metrics);

    // A lone reaction never collapses into a "+1": it overhangs the bubble
    // instead, which still reads as the reaction it is.
    if (!limit.isFinite || full <= limit || count == 1) {
      return [for (final group in ordered) _chip(group)];
    }

    // Reserve room for the widest label the trailing chip can take.
    final reserve = tokens.spacing + metrics.overflowWidth(count);
    var used = 0.0;
    var shown = 0;
    for (var i = 0; i < count; i++) {
      final add = metrics.widths[i] + (shown > 0 ? tokens.spacing : 0);
      if (used + add + reserve > limit) break;
      used += add;
      shown++;
    }

    // Too narrow for even one emoji beside the trailing chip collapses every
    // reaction into it.
    return [
      for (var i = 0; i < shown; i++) _chip(ordered[i]),
      ReactionChip.overflow(
        tokens: chipTokens,
        count: count - shown,
        onTap: onTapOverflow,
      ),
    ];
  }

  Widget _chip(ReactionGroup group) => ReactionChip(
    tokens: chipTokens,
    emoji: group.emoji,
    count: group.count,
    selected: group.mine,
    onTap: onTapEmoji == null ? null : () => onTapEmoji!(group.emoji),
  );

  Widget _row(List<Widget> chips, {required bool bounded}) {
    final row = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var i = 0; i < chips.length; i++) ...[
          if (i > 0) SizedBox(width: tokens.spacing),
          chips[i],
        ],
      ],
    );
    if (!bounded) return row;
    // Even the collapsed pill can be wider than a very narrow bubble. Let it
    // overhang the bubble's edge rather than force-fitting the run into a width
    // it can't take.
    return OverflowBox(
      minWidth: 0,
      maxWidth: double.infinity,
      alignment: Alignment.centerLeft,
      child: row,
    );
  }
}

/// Chip footprints, measured once per build.
///
/// They depend only on the reaction data and the text scaler, not on the width
/// the strip is given, so packing against a new width never measures again.
@immutable
class _ChipMetrics {
  const _ChipMetrics._({
    required this.widths,
    required this.height,
    required this.chrome,
    required this.countStyle,
    required this.scaler,
    required this.direction,
  });

  factory _ChipMetrics.measure(
    BuildContext context,
    ReactionChipTokens tokens,
    List<ReactionGroup> groups,
  ) {
    // We merge with the ambient style the way the chips' own Text and
    // CenteredEmoji do, so the widths come out of the font they render with.
    final ambient = DefaultTextStyle.of(context).style;
    final glyphStyle = ambient.merge(ReactionChip.glyphStyle());
    final countStyle = ambient.merge(ReactionChip.countStyle());
    final scaler = MediaQuery.textScalerOf(context);
    final direction = Directionality.of(context);

    final chrome = tokens.padding.horizontal + ReactionChipTokens.cropWidth * 2;
    final widths = <double>[];
    var content = 0.0;
    for (final group in groups) {
      final glyph = _measure(group.emoji, glyphStyle, scaler, direction);
      var width = chrome + glyph.width;
      content = math.max(content, glyph.height);
      if (group.count > 1) {
        final label = _measure('${group.count}', countStyle, scaler, direction);
        width += ReactionChipTokens.countGap + label.width;
        content = math.max(content, label.height);
      }
      widths.add(width);
    }

    final pill = math.max(tokens.minHeight, tokens.padding.vertical + content);
    return _ChipMetrics._(
      widths: widths,
      height: pill + ReactionChipTokens.cropWidth * 2,
      chrome: chrome,
      countStyle: countStyle,
      scaler: scaler,
      direction: direction,
    );
  }

  final List<double> widths;

  /// Height of a chip box, crop ring included.
  final double height;

  /// Horizontal chrome of a chip: the pill's padding plus its crop ring.
  final double chrome;

  final TextStyle countStyle;
  final TextScaler scaler;
  final TextDirection direction;

  /// The strip's minimal representation: the lone chip when there's a single
  /// group, else the pill everything collapses into.
  double get minimalWidth =>
      widths.length == 1 ? widths.first : overflowWidth(widths.length);

  double overflowWidth(int hidden) =>
      chrome + _measure('+$hidden', countStyle, scaler, direction).width;
}

Size _measure(
  String text,
  TextStyle style,
  TextScaler scaler,
  TextDirection direction,
) {
  final painter = TextPainter(
    text: TextSpan(text: text, style: style),
    textDirection: direction,
    textScaler: scaler,
    maxLines: 1,
  )..layout();
  return painter.size;
}

/// Reports the strip's footprint as its intrinsic size.
///
/// The run packs into whatever width the message gives it, so the width it
/// happens to be laid out at answers nothing useful. What a host needs is
/// either end of the range: the smallest the strip can get, one chip or the
/// pill everything collapses into, which is what it has to fit beside whatever
/// shares its line, and the widest, the full run, which is what something
/// sharing that line has to clear. Packing itself runs against the laid-out
/// width, and a [LayoutBuilder] has no intrinsics to offer, so we measure the
/// numbers up front and answer them here.
class _StripFootprint extends SingleChildRenderObjectWidget {
  const _StripFootprint({
    required this.minimalWidth,
    required this.runWidth,
    required this.height,
    required Widget super.child,
  });

  final double minimalWidth;
  final double runWidth;
  final double height;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderStripFootprint(minimalWidth, runWidth, height);

  @override
  void updateRenderObject(BuildContext context, RenderObject renderObject) {
    (renderObject as _RenderStripFootprint)
      ..minimalWidth = minimalWidth
      ..runWidth = runWidth
      ..height = height;
  }
}

class _RenderStripFootprint extends RenderProxyBox {
  _RenderStripFootprint(this._minimalWidth, this._runWidth, this._height);

  double _minimalWidth;
  set minimalWidth(double value) {
    if (value == _minimalWidth) return;
    _minimalWidth = value;
    markNeedsLayout();
  }

  double _runWidth;
  set runWidth(double value) {
    if (value == _runWidth) return;
    _runWidth = value;
    markNeedsLayout();
  }

  double _height;
  set height(double value) {
    if (value == _height) return;
    _height = value;
    markNeedsLayout();
  }

  @override
  double computeMinIntrinsicWidth(double height) => _minimalWidth;

  @override
  double computeMaxIntrinsicWidth(double height) => _runWidth;

  @override
  double computeMinIntrinsicHeight(double width) => _height;

  @override
  double computeMaxIntrinsicHeight(double width) => _height;
}
