// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/emoji/centered_emoji.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/reaction_bar/reaction_bar_tokens.dart';
import 'package:air/platform/haptics.dart';
import 'package:flutter/widgets.dart';

/// The quick-reaction bar: a floating row of common emojis with a trailing
/// button that escalates to the full picker.
///
/// The first form a reaction picker takes, surfaced on long press, on hover, or
/// from a message's context menu. The emojis arrive ready to send, skin tone
/// already applied, so the bar only has to render and report them.
class ReactionBar extends StatelessWidget {
  const ReactionBar({
    super.key,
    required this.tokens,
    required this.emojis,
    required this.onPick,
    required this.onMore,
  });

  final ReactionBarTokens tokens;
  final List<String> emojis;

  final void Function(String emoji) onPick;
  final VoidCallback onMore;

  /// Style an emoji renders at in a bar with these [tokens].
  static TextStyle glyphStyle(ReactionBarTokens tokens) =>
      TextStyle(fontSize: tokens.glyphSize);

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    // Kick the ink measurements off now, so the glyphs settle while the bar's
    // open transition is still running rather than snapping into place after.
    CenteredEmoji.warmUp(context, emojis, glyphStyle(tokens));

    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.backgroundElevated.primary,
        borderRadius: BorderRadius.circular(ReactionBarTokens.radius),
        boxShadow: Effect.elevation(ReactionBarTokens.elevation),
      ),
      child: Padding(
        padding: tokens.containerPadding,
        child: Row(
          mainAxisSize: .min,
          children: [
            for (final emoji in emojis)
              _PickButton(
                tokens: tokens,
                emoji: emoji,
                onTap: () => onPick(emoji),
              ),
            ButtonIcon(
              variant: ButtonIconVariant.transparent,
              icon: AppIconType.plus,
              size: tokens.moreSize,
              iconSize: tokens.moreIconSize,
              iconColor: palette.text.secondary,
              hitTargetSize: tokens.itemSize,
              onPressed: onMore,
            ),
          ],
        ),
      ),
    );
  }
}

/// One emoji in the bar. Pulses on pick, so the choice registers on a bar
/// that's already dismissing itself.
class _PickButton extends StatefulWidget {
  const _PickButton({
    required this.tokens,
    required this.emoji,
    required this.onTap,
  });

  final ReactionBarTokens tokens;
  final String emoji;
  final VoidCallback onTap;

  @override
  State<_PickButton> createState() => _PickButtonState();
}

class _PickButtonState extends State<_PickButton>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pulse;
  late final Animation<double> _scale;

  @override
  void initState() {
    super.initState();
    _pulse = AnimationController(
      vsync: this,
      duration: Effect.duration(ReactionBarTokens.pickMotion),
    );
    _scale = TweenSequence<double>([
      TweenSequenceItem(
        tween: Tween(begin: 1.0, end: ReactionBarTokens.pickScale),
        weight: ReactionBarTokens.pickGrowWeight.toDouble(),
      ),
      TweenSequenceItem(
        tween: Tween(begin: ReactionBarTokens.pickScale, end: 1.0),
        weight: ReactionBarTokens.pickSettleWeight.toDouble(),
      ),
    ]).animate(_pulse);
  }

  @override
  void dispose() {
    _pulse.dispose();
    super.dispose();
  }

  void _handleTap() {
    AppHaptics.confirm();
    _pulse.forward(from: 0);
    widget.onTap();
  }

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: .opaque,
        onTap: _handleTap,
        child: SizedBox.square(
          dimension: widget.tokens.itemSize,
          child: Center(
            child: ScaleTransition(
              scale: _scale,
              child: CenteredEmoji(
                emoji: widget.emoji,
                style: ReactionBar.glyphStyle(widget.tokens),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Places the bar centered on an anchor and [gap] above it, flipping below when
/// there's no room above, and clamped into [safeArea] either way.
///
/// A message near an edge of the screen would otherwise push the bar off it,
/// and a bar the user can't reach is a bar that can't be used.
class ReactionBarAnchorLayout extends SingleChildLayoutDelegate {
  const ReactionBarAnchorLayout({
    required this.anchorRect,
    required this.safeArea,
    this.gap = ReactionBarTokens.anchorGap,
  });

  /// The thing the bar belongs to, in the coordinates of the box being laid
  /// out.
  final Rect anchorRect;

  /// Region the bar has to stay inside, the system insets included.
  final EdgeInsets safeArea;

  final double gap;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      // Let the bar size to its content instead of filling the overlay.
      constraints.loosen();

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    var dx = anchorRect.center.dx - childSize.width / 2;

    final above = anchorRect.top - gap - childSize.height;
    final below = anchorRect.bottom + gap;
    var dy = above >= safeArea.top ? above : below;

    final maxX = (size.width - safeArea.right - childSize.width)
        .clamp(safeArea.left, size.width)
        .toDouble();
    final maxY = (size.height - safeArea.bottom - childSize.height)
        .clamp(safeArea.top, size.height)
        .toDouble();
    dx = dx.clamp(safeArea.left, maxX).toDouble();
    dy = dy.clamp(safeArea.top, maxY).toDouble();
    return Offset(dx, dy);
  }

  @override
  bool shouldRelayout(ReactionBarAnchorLayout oldDelegate) =>
      oldDelegate.anchorRect != anchorRect ||
      oldDelegate.safeArea != safeArea ||
      oldDelegate.gap != gap;
}
