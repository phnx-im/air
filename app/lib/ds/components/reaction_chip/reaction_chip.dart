// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/emoji/centered_emoji.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/components/reaction_chip/reaction_chip_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/platform/haptics.dart';
import 'package:flutter/widgets.dart';

/// A single reaction pill: one emoji, plus a count once more than one person
/// reacted with it.
///
/// Two shapes:
///   * [ReactionChip] -- the emoji, marked [selected] when the reaction is the
///     current user's own.
///   * [ReactionChip.overflow] -- a `+N` pill standing in for the reactions
///     that didn't fit, so a narrow bubble keeps the most popular ones.
///
/// We wrap the pill in a ring of the color behind the message, which crops
/// it out of the bubble it overlaps. Everything else about the chip's placement
/// belongs to whatever lays the run of chips out.
class ReactionChip extends StatelessWidget {
  const ReactionChip({
    super.key,
    required this.tokens,
    required this.emoji,
    this.count = 1,
    this.selected = false,
    this.onTap,
  }) : overflowCount = null;

  const ReactionChip.overflow({
    super.key,
    required this.tokens,
    required int count,
    this.onTap,
  }) : overflowCount = count,
       emoji = '',
       count = 0,
       selected = false;

  final ReactionChipTokens tokens;
  final String emoji;

  /// People who reacted with this emoji. A count of 1 hides the number.
  final int count;

  /// Whether the current user is one of the reactors. Picks the heavier fill,
  /// regardless of whose message the chip sits under.
  final bool selected;

  /// Reactions collapsed into this pill. Non-null only on the overflow shape.
  final int? overflowCount;

  final VoidCallback? onTap;

  /// Style the emoji renders at. Pinned to a 100% line so the pill hugs the
  /// glyph instead of the glyph's leading. Exposed so a host can warm the ink
  /// measurements up ahead of the first chip, see [CenteredEmoji.warmUp].
  static TextStyle glyphStyle() => typeScale.body.regular.style(tight: true);

  /// Style the count and the `+N` label render at, exposed for the reason
  /// [glyphStyle] gives. The color is optional so that code measuring a chip
  /// doesn't have to resolve a palette first.
  static TextStyle countStyle([Color? color]) => typeScale.body.mini.style(
    color: color,
    weight: Weight.emphasized,
    tight: true,
  );

  /// The color the crop ring paints in: whatever the message list itself paints
  /// on, so the ring reads as a gap rather than an outline.
  static Color cropColor(BuildContext context) =>
      PanelSurface.maybeOf(context) ??
      SemanticPalette.of(context).backgroundBase.primary;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final hidden = overflowCount;

    final content = hidden != null
        ? Text('+$hidden', style: countStyle(palette.text.tertiary))
        : Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              // Ink-centered rather than a raw Text: on iOS the emoji glyph
              // isn't centered within its own line box, see [CenteredEmoji].
              CenteredEmoji(emoji: emoji, style: glyphStyle()),
              if (count > 1) ...[
                const SizedBox(width: ReactionChipTokens.countGap),
                Text('$count', style: countStyle(palette.text.tertiary)),
              ],
            ],
          );

    // The crop ring stays outside the StateLayer so the wash never tints it
    // and it keeps reading as a gap.
    return DecoratedBox(
      decoration: BoxDecoration(
        color: cropColor(context),
        borderRadius: BorderRadius.circular(ReactionChipTokens.radius),
      ),
      child: Padding(
        padding: const EdgeInsets.all(ReactionChipTokens.cropWidth),
        child: StateLayer(
          borderRadius: ReactionChipTokens.radius,
          surface: selected ? palette.fill.primary : palette.fill.tertiary,
          onTap: onTap == null ? null : () => _handleTap(onTap!),
          background: DecoratedBox(
            decoration: BoxDecoration(
              // The collapsed pill is nobody's own reaction, so it always
              // takes the lighter fill.
              color: selected ? palette.fill.primary : palette.fill.tertiary,
              borderRadius: BorderRadius.circular(ReactionChipTokens.radius),
            ),
          ),
          child: Container(
            constraints: BoxConstraints(minHeight: tokens.minHeight),
            padding: tokens.padding,
            alignment: Alignment.center,
            child: content,
          ),
        ),
      ),
    );
  }

  void _handleTap(VoidCallback handler) {
    AppHaptics.selection();
    handler();
  }
}
