// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/core/core.dart';
import 'package:air/ds/components/reaction_chip/reaction_chip_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/reaction_strip/reaction_strip.dart';
import 'package:air/ds/patterns/reaction_strip/reaction_strip_tokens.dart';
import 'package:air/features/message_list/message_reactions.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// Clearance the stamp keeps from the run of chips where they share a line.
const double _stampGap = S.s8;

/// One message's line: the bubble, the reaction chips cropped into its bottom
/// edge, the stamp under it, and the hover affordance beside it.
///
/// The chips ride up over the bubble's bottom edge, so the band reserves their
/// height below it and the message that follows keeps its distance. The
/// reserve and the chips animate in when the first reaction arrives and out
/// when the last one is removed; a tile that mounts with reactions renders the
/// settled state without animating.
///
/// Where the bubble is wide enough to carry the run and the stamp side by
/// side, the stamp keeps the place it would have had without any reactions and
/// the chips fill the space beside it, which spares the row a line of chrome.
/// Only an own message can: an incoming one anchors both to the leading edge,
/// where they would run into each other.
class MessageBand extends StatefulWidget {
  const MessageBand({
    super.key,
    required this.outgoing,
    required this.bubble,
    required this.stamp,
    required this.affordance,
    required this.reactions,
    required this.ownUserId,
    required this.onTapReaction,
  });

  /// Own message. Hangs the bubble, and with it the stamp, off the trailing
  /// edge, which is what leaves the chips a line to share.
  final bool outgoing;

  final Widget bubble;

  /// The stamp under the bubble, on the rows that carry one.
  final Widget? stamp;

  /// The hover buttons beside the bubble, centered on it rather than on the
  /// band: they act on the message, not on what is reported under it.
  final Widget? affordance;

  final List<UiReaction> reactions;
  final UiUserId ownUserId;

  /// Reveals who reacted. Null stands for the collapsed `+N` chip, which
  /// belongs to no single emoji.
  final void Function(String? emoji) onTapReaction;

  @override
  State<MessageBand> createState() => _MessageBandState();
}

class _MessageBandState extends State<MessageBand>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;
  late final Animation<double> _reveal;
  late final Animation<double> _chipScale;

  /// Last non-empty reactions, kept while the chips animate out.
  List<UiReaction> _reactions = const [];

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: Effect.duration(MotionPreset.short),
      value: widget.reactions.isEmpty ? 0.0 : 1.0,
    );
    _controller.addStatusListener(_onStatusChanged);
    _reveal = CurvedAnimation(
      parent: _controller,
      curve: Effect.easeOutQuart,
      // Mirrored so removal collapses the reserve the way it opened.
      reverseCurve: const FlippedCurve(Effect.easeOutQuart),
    );
    _chipScale = Tween<double>(begin: 0.6, end: 1.0).animate(_reveal);
    if (widget.reactions.isNotEmpty) {
      _reactions = widget.reactions;
    }
  }

  void _onStatusChanged(AnimationStatus status) {
    // Drop the stale chips once the exit animation settled.
    if (status == AnimationStatus.dismissed && widget.reactions.isEmpty) {
      setState(() => _reactions = const []);
    }
  }

  @override
  void didUpdateWidget(covariant MessageBand oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.reactions.isNotEmpty) {
      _reactions = widget.reactions;
      _controller.forward();
    } else if (oldWidget.reactions.isNotEmpty) {
      _controller.reverse();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tokens = ReactionStripTokens.current;

    return _BandLayout(
      reveal: _reveal,
      reserve: reactionsReservedBelow(context, true),
      lift: tokens.lift,
      outgoing: widget.outgoing,
      bubble: widget.bubble,
      chips: _reactions.isEmpty ? null : _chips(tokens),
      stamp: widget.stamp,
      affordance: widget.affordance,
    );
  }

  Widget _chips(ReactionStripTokens tokens) => IgnorePointer(
    ignoring: widget.reactions.isEmpty,
    child: FadeTransition(
      opacity: _reveal,
      child: ScaleTransition(
        scale: _chipScale,
        alignment: Alignment.bottomLeft,
        child: ReactionStrip(
          tokens: tokens,
          chipTokens: ReactionChipTokens.current,
          groups: [
            for (final reaction in _reactions)
              ReactionGroup(
                emoji: reaction.emoji,
                count: reaction.users.length,
                mine: reaction.users.contains(widget.ownUserId),
              ),
          ],
          onTapEmoji: widget.onTapReaction,
          onTapOverflow: () => widget.onTapReaction(null),
        ),
      ),
    ),
  );
}

enum _BandSlot { affordance, bubble, chips, stamp }

class _BandLayout
    extends SlottedMultiChildRenderObjectWidget<_BandSlot, RenderBox> {
  const _BandLayout({
    required this.reveal,
    required this.reserve,
    required this.lift,
    required this.outgoing,
    required this.bubble,
    required this.chips,
    required this.stamp,
    required this.affordance,
  });

  /// How far the reserve below the bubble is open, from 0 to 1.
  final Animation<double> reveal;

  /// Height the open reserve takes.
  final double reserve;

  /// See [ReactionStripTokens.lift].
  final double lift;

  final bool outgoing;
  final Widget bubble;
  final Widget? chips;
  final Widget? stamp;
  final Widget? affordance;

  @override
  Iterable<_BandSlot> get slots => _BandSlot.values;

  @override
  Widget? childForSlot(_BandSlot slot) => switch (slot) {
    _BandSlot.affordance => affordance,
    _BandSlot.bubble => bubble,
    _BandSlot.chips => chips,
    _BandSlot.stamp => stamp,
  };

  @override
  _RenderBand createRenderObject(BuildContext context) =>
      _RenderBand(reveal, reserve, lift, outgoing);

  @override
  void updateRenderObject(BuildContext context, _RenderBand renderObject) {
    renderObject
      ..reveal = reveal
      ..reserve = reserve
      ..lift = lift
      ..outgoing = outgoing;
  }
}

class _RenderBand extends RenderBox
    with SlottedContainerRenderObjectMixin<_BandSlot, RenderBox> {
  _RenderBand(this._reveal, this._reserve, this._lift, this._outgoing);

  Animation<double> _reveal;
  set reveal(Animation<double> value) {
    if (identical(value, _reveal)) return;
    if (attached) {
      _reveal.removeListener(markNeedsLayout);
      value.addListener(markNeedsLayout);
    }
    _reveal = value;
    markNeedsLayout();
  }

  double _reserve;
  set reserve(double value) {
    if (value == _reserve) return;
    _reserve = value;
    markNeedsLayout();
  }

  double _lift;
  set lift(double value) {
    if (value == _lift) return;
    _lift = value;
    markNeedsLayout();
  }

  bool _outgoing;
  set outgoing(bool value) {
    if (value == _outgoing) return;
    _outgoing = value;
    markNeedsLayout();
  }

  @override
  void attach(PipelineOwner owner) {
    super.attach(owner);
    _reveal.addListener(markNeedsLayout);
  }

  @override
  void detach() {
    _reveal.removeListener(markNeedsLayout);
    super.detach();
  }

  RenderBox? get _bubble => childForSlot(_BandSlot.bubble);
  RenderBox? get _chips => childForSlot(_BandSlot.chips);
  RenderBox? get _stamp => childForSlot(_BandSlot.stamp);
  RenderBox? get _affordance => childForSlot(_BandSlot.affordance);

  /// Back to front: the chips are cropped into the bubble they overlap.
  @override
  Iterable<RenderBox> get children =>
      [_affordance, _bubble, _chips, _stamp].nonNulls;

  @override
  void setupParentData(RenderBox child) {
    if (child.parentData is! BoxParentData) {
      child.parentData = BoxParentData();
    }
  }

  @override
  void performLayout() {
    final loose = constraints.loosen();
    final bubble = _bubble!;
    bubble.layout(loose, parentUsesSize: true);
    final bubbleSize = bubble.size;

    // The reserve as far as it has opened, which is where the foot of the band
    // sits while the chips animate in or out.
    final reserve = _reserve * _reveal.value;

    final chips = _chips;
    var runWidth = 0.0;
    if (chips != null) {
      // The run packs into the bubble's width and answers for the width it
      // takes with nothing collapsed, see [ReactionStrip].
      chips.layout(
        BoxConstraints.tightFor(width: bubbleSize.width),
        parentUsesSize: true,
      );
      runWidth = chips.getMaxIntrinsicWidth(double.infinity);
    }

    final stamp = _stamp;
    stamp?.layout(loose, parentUsesSize: true);
    final stampSize = stamp?.size ?? Size.zero;

    final affordance = _affordance;
    affordance?.layout(loose, parentUsesSize: true);
    final affordanceSize = affordance?.size ?? Size.zero;

    // The stamp hangs off the bubble's trailing edge and the run starts at its
    // leading one, so on an own message the two only meet where the bubble is
    // too narrow to carry both.
    final shared =
        _outgoing &&
        chips != null &&
        stamp != null &&
        runWidth + _stampGap + stampSize.width <= bubbleSize.width;

    final affordanceTop = math.max(
      0.0,
      (bubbleSize.height - affordanceSize.height) / 2,
    );

    final stampTop = bubbleSize.height + (shared ? 0.0 : reserve);
    // A shared line hands back the space the run leaves at the foot of the
    // reserve, so the row ends at whichever of the two reaches lower.
    final foot = math.max(
      bubbleSize.height + reserve - (shared ? _lift : 0.0),
      stampTop + stampSize.height,
    );

    size = constraints.constrain(
      Size(
        math.max(affordanceSize.width + bubbleSize.width, stampSize.width),
        math.max(foot, affordanceTop + affordanceSize.height),
      ),
    );

    final bubbleLeft = _outgoing ? size.width - bubbleSize.width : 0.0;
    _place(bubble, Offset(bubbleLeft, 0));
    if (affordance != null) {
      _place(
        affordance,
        Offset(
          _outgoing
              ? bubbleLeft - affordanceSize.width
              : bubbleLeft + bubbleSize.width,
          affordanceTop,
        ),
      );
    }
    if (chips != null) {
      _place(
        chips,
        Offset(bubbleLeft, bubbleSize.height + reserve - chips.size.height),
      );
    }
    if (stamp != null) {
      _place(
        stamp,
        Offset(_outgoing ? size.width - stampSize.width : 0, stampTop),
      );
    }
  }

  void _place(RenderBox child, Offset offset) {
    (child.parentData! as BoxParentData).offset = offset;
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    for (final child in children) {
      final data = child.parentData! as BoxParentData;
      context.paintChild(child, offset + data.offset);
    }
  }

  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) {
    for (final child in children.toList().reversed) {
      final data = child.parentData! as BoxParentData;
      final hit = result.addWithPaintOffset(
        offset: data.offset,
        position: position,
        hitTest: (result, transformed) =>
            child.hitTest(result, position: transformed),
      );
      if (hit) return true;
    }
    return false;
  }
}
