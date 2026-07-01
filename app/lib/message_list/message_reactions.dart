// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/user/users_cubit.dart';
import 'package:flutter/material.dart';

import 'package:air/core/core.dart';
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'emoji_repository.dart';

/// Compact reaction chip metrics
const double _chipSpacing = Spacing.px8;

/// Fixed height of a reaction chip
const double reactionChipHeight = 28;

/// How far the chips overlap the bottom border of the message bubble
const double reactionsOverlap = Spacing.px4;

/// Aligning the chips with the bubble's text rather than its rounded corner
const double reactionsHorizontalInset = Spacing.px16;

/// Gap below the chips before the timestamp / next message.
const double _reactionsGapBelow = Spacing.px4;

/// Vertical space [BubbleWithReactions] reserves below the bubble for the chips
/// that overlap its bottom edge (zero when there are no reactions). Used by
/// callers to align elements to the bubble rather than the bubble + chips.
double reactionsReservedBelow(bool hasReactions) => hasReactions
    ? reactionChipHeight - reactionsOverlap + _reactionsGapBelow
    : 0;

/// Overlays [MessageReactions] onto the bottom edge of [bubble].
///
/// The chips overlap the bubble's bottom by [reactionsOverlap] and the layout
/// reserves the chips' protruding height (plus a small gap) below the bubble so
/// following messages don't collide. Returns [bubble] unchanged when there are
/// no reactions.
class BubbleWithReactions extends StatelessWidget {
  const BubbleWithReactions({
    super.key,
    required this.bubble,
    required this.reactions,
    required this.isSender,
    required this.ownUserId,
    this.onTap,
  });

  final Widget bubble;
  final List<UiReaction> reactions;
  final bool isSender;
  final UiUserId ownUserId;
  final void Function(String? emoji)? onTap;

  @override
  Widget build(BuildContext context) {
    if (reactions.isEmpty) {
      return bubble;
    }
    return Stack(
      clipBehavior: Clip.none,
      // alignment: AlignmentGeometry.ce,
      children: [
        Padding(
          padding: const EdgeInsets.only(
            bottom: reactionChipHeight - reactionsOverlap + _reactionsGapBelow,
          ),
          child: bubble,
        ),
        Positioned(
          bottom: _reactionsGapBelow,
          left: reactionsHorizontalInset,
          right: reactionsHorizontalInset,
          child: MessageReactions(
            reactions: reactions,
            isSender: isSender,
            ownUserId: ownUserId,
            onTap: onTap,
          ),
        ),
      ],
    );
  }
}

/// A row of emoji reaction chips, rendered overlapping the bottom edge of a
/// message bubble.
///
/// Aligns to the bubble's side ([isSender]), shows a numeric count only when an
/// emoji has 2+ reactors, and highlights the current user's own reactions with
/// an accent-colored border. [onTap] is invoked with the tapped emoji (used to
/// show who reacted); there is no tap-to-remove.
class MessageReactions extends StatelessWidget {
  const MessageReactions({
    super.key,
    required this.reactions,
    required this.isSender,
    required this.ownUserId,
    this.onTap,
  });

  final List<UiReaction> reactions;
  final bool isSender;
  final UiUserId ownUserId;
  final void Function(String? emoji)? onTap;

  @override
  Widget build(BuildContext context) {
    if (reactions.isEmpty) {
      return const SizedBox.shrink();
    }

    // Highest-count first (stable for ties), so a narrow bubble keeps the most
    // popular reactions and collapses the rest into a "+N" overflow chip.
    final indexed =
        [
          for (var i = 0; i < reactions.length; i++)
            (reaction: reactions[i], i: i),
        ]..sort((a, b) {
          final byCount = b.reaction.users.length.compareTo(
            a.reaction.users.length,
          );
          return byCount != 0 ? byCount : a.i.compareTo(b.i);
        });
    final ordered = [for (final e in indexed) e.reaction];

    final scaler = MediaQuery.textScalerOf(context);
    final emojiStyle = TextStyle(fontSize: FontSizes.small2.size, height: 1.0);
    final countStyle = TextStyle(fontSize: FontSizes.small3.size, height: 1.0);
    double measure(String text, TextStyle style) {
      final painter = TextPainter(
        text: TextSpan(text: text, style: style),
        textDirection: TextDirection.ltr,
        textScaler: scaler,
      )..layout();
      return painter.width;
    }

    // Horizontal chrome of a chip: padding (both sides) + border (both sides).
    const chipChrome = Spacing.px8 * 2 + 2;
    double chipWidth(UiReaction reaction) {
      var width = chipChrome + measure(reaction.emoji, emojiStyle);
      if (reaction.users.length >= 2) {
        width += Spacing.px4 + measure('${reaction.users.length}', countStyle);
      }
      return width;
    }

    Widget chipFor(UiReaction reaction) => _ReactionChip(
      reaction: reaction,
      isSender: isSender,
      isMine: reaction.users.contains(ownUserId),
      onTap: onTap == null ? null : () => onTap!(reaction.emoji),
    );

    return LayoutBuilder(
      builder: (context, constraints) {
        final maxWidth = constraints.maxWidth;
        final widths = [for (final reaction in ordered) chipWidth(reaction)];
        final count = ordered.length;

        var fullWidth = 0.0;
        for (var i = 0; i < count; i++) {
          fullWidth += widths[i] + (i > 0 ? _chipSpacing : 0);
        }

        final List<Widget> chips;
        if (!maxWidth.isFinite || fullWidth <= maxWidth) {
          chips = [for (final reaction in ordered) chipFor(reaction)];
        } else {
          // Reserve room for the overflow chip ("+N" upper bound).
          final overflowReserve =
              _chipSpacing + chipChrome + measure('+$count', emojiStyle);
          var used = 0.0;
          var shown = 0;
          for (var i = 0; i < count; i++) {
            final add = widths[i] + (shown > 0 ? _chipSpacing : 0);
            if (used + add + overflowReserve <= maxWidth) {
              used += add;
              shown++;
            } else {
              break;
            }
          }
          if (shown == 0) shown = 1; // always keep the highest-count emoji
          chips = [
            for (var i = 0; i < shown; i++) chipFor(ordered[i]),
            _OverflowChip(
              count: count - shown,
              isSender: isSender,
              onTap: onTap == null ? null : () => onTap!(null),
            ),
          ];
        }

        final children = <Widget>[];
        for (var i = 0; i < chips.length; i++) {
          if (i > 0) children.add(const SizedBox(width: _chipSpacing));
          children.add(chips[i]);
        }
        return Row(
          mainAxisAlignment: isSender
              ? MainAxisAlignment.start
              : MainAxisAlignment.end,
          children: children,
        );
      },
    );
  }
}

/// The curated quick-reaction set shown in the [QuickReactionBar]. Skinnable
/// entries follow the user's default skin tone.
const List<({String emoji, bool skinnable})> quickReactionEmojis = [
  (emoji: '👍', skinnable: true),
  (emoji: '❤️', skinnable: false),
  (emoji: '😂', skinnable: false),
  (emoji: '😮', skinnable: false),
  (emoji: '😢', skinnable: false),
  (emoji: '🙏', skinnable: true),
];

const double _quickGlyphSize = 28;
const double _quickTapSize = 44;
const double _quickMoreSize = 36;

String _applyQuickTone(
  ({String emoji, bool skinnable}) item,
  EmojiSkinVariation tone,
) {
  if (!item.skinnable || tone == EmojiSkinVariation.none) {
    return item.emoji;
  }
  return '${item.emoji}${tone.modifier}';
}

/// A horizontal pill bar of common quick reactions plus a trailing "+" that
/// opens the full emoji picker.
///
/// [onReact] receives the tone-applied emoji; [onMore] opens the picker.
class QuickReactionBar extends StatelessWidget {
  const QuickReactionBar({
    super.key,
    required this.onReact,
    required this.onMore,
    this.skinTone = EmojiSkinVariation.none,
  });

  final void Function(String emoji) onReact;
  final VoidCallback onMore;
  final EmojiSkinVariation skinTone;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.px8,
        vertical: Spacing.px8,
      ),
      decoration: BoxDecoration(
        color: colors.backgroundElevated.primary,
        borderRadius: BorderRadius.circular((_quickTapSize + Spacing.px8) / 2),
        boxShadow: const [
          BoxShadow(
            color: Color(0x33000000),
            blurRadius: 16,
            offset: Offset(0, 4),
          ),
        ],
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final item in quickReactionEmojis)
            _QuickReactionButton(
              emoji: _applyQuickTone(item, skinTone),
              onTap: () => onReact(_applyQuickTone(item, skinTone)),
            ),
          GlassCircleButton(
            onPressed: onMore,
            size: _quickMoreSize,
            hitTargetSize: _quickTapSize,
            enableBackdropBlur: false,
            shadows: const [],
            color: colors.backgroundBase.secondary,
            icon: AppIcon.plus(size: 18, color: colors.text.secondary),
          ),
        ],
      ),
    );
  }
}

/// Shows the [QuickReactionBar] as a small popover anchored above [anchorRect]
/// (or below it when there isn't room above), aligned to the message's side.
///
/// [onReact] receives the chosen quick emoji; [onMore] is invoked when the user
/// taps "+". Both dismiss the popover first.
Future<void> showQuickReactionMenu({
  required BuildContext context,
  required Rect anchorRect,
  required bool alignEnd,
  required EmojiSkinVariation skinTone,
  required void Function(String emoji) onReact,
  required VoidCallback onMore,
}) {
  return showGeneralDialog(
    context: context,
    barrierDismissible: true,
    barrierColor: Colors.transparent,
    barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    transitionDuration: const Duration(milliseconds: 150),
    pageBuilder: (context, animation, secondaryAnimation) =>
        const SizedBox.shrink(),
    transitionBuilder: (dialogContext, animation, secondaryAnimation, child) {
      final curved = CurvedAnimation(
        parent: animation,
        curve: Curves.easeOutCubic,
        reverseCurve: Curves.easeInCubic,
      );
      return _QuickReactionMenuOverlay(
        animation: curved,
        anchorRect: anchorRect,
        alignEnd: alignEnd,
        skinTone: skinTone,
        onReact: (emoji) {
          Navigator.of(dialogContext).pop();
          onReact(emoji);
        },
        onMore: () {
          Navigator.of(dialogContext).pop();
          onMore();
        },
      );
    },
  );
}

/// Approximate height of the quick-reaction bar, used to position it above the
/// anchored message.
const double quickReactionBarHeight = _quickTapSize + Spacing.px8;

class _QuickReactionMenuOverlay extends StatelessWidget {
  const _QuickReactionMenuOverlay({
    required this.animation,
    required this.anchorRect,
    required this.alignEnd,
    required this.skinTone,
    required this.onReact,
    required this.onMore,
  });

  final Animation<double> animation;
  final Rect anchorRect;
  final bool alignEnd;
  final EmojiSkinVariation skinTone;
  final void Function(String emoji) onReact;
  final VoidCallback onMore;

  @override
  Widget build(BuildContext context) {
    final mediaQuery = MediaQuery.of(context);
    final safeTop = mediaQuery.padding.top + Spacing.px8;
    final safeBottom = mediaQuery.padding.bottom + Spacing.px8;

    // Prefer placing the bar above the message; fall back to below it.
    final aboveTop = anchorRect.top - Spacing.px8 - quickReactionBarHeight;
    final placeAbove = aboveTop >= safeTop;
    final top = placeAbove
        ? aboveTop
        : (anchorRect.bottom + Spacing.px8).clamp(
            safeTop,
            mediaQuery.size.height - safeBottom - quickReactionBarHeight,
          );

    final bar = FadeTransition(
      opacity: animation,
      child: ScaleTransition(
        scale: Tween<double>(begin: 0.92, end: 1.0).animate(animation),
        alignment: alignEnd ? Alignment.bottomRight : Alignment.bottomLeft,
        child: QuickReactionBar(
          onReact: onReact,
          onMore: onMore,
          skinTone: skinTone,
        ),
      ),
    );

    return Stack(
      children: [
        Positioned(
          top: top,
          left: alignEnd ? null : anchorRect.left,
          right: alignEnd ? (mediaQuery.size.width - anchorRect.right) : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacing.px8),
            child: bar,
          ),
        ),
      ],
    );
  }
}

class _QuickReactionButton extends StatelessWidget {
  const _QuickReactionButton({required this.emoji, required this.onTap});

  final String emoji;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: SizedBox(
          width: _quickTapSize,
          height: _quickTapSize,
          child: Center(
            child: FittedBox(
              fit: .contain,
              child: Text(
                emoji,
                style: const TextStyle(
                  fontSize: _quickGlyphSize,
                  overflow: .visible,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ReactionChip extends StatelessWidget {
  const _ReactionChip({
    required this.reaction,
    required this.isSender,
    required this.isMine,
    this.onTap,
  });

  final UiReaction reaction;
  final bool isSender;
  final bool isMine;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final count = reaction.users.length;

    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Container(
          height: reactionChipHeight,
          padding: const EdgeInsets.symmetric(
            horizontal: Spacing.px8,
            vertical: Spacing.px4,
          ),
          decoration: BoxDecoration(
            color: isSender
                ? colors.message.selfBackground
                : colors.message.otherBackground,
            borderRadius: BorderRadius.circular(reactionChipHeight / 2),
            border: Border.all(color: colors.backgroundBase.primary),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                reaction.emoji,
                style: TextStyle(fontSize: FontSizes.small2.size, height: 1.0),
              ),
              if (count >= 2) ...[
                const SizedBox(width: Spacing.px4),
                Text(
                  '$count',
                  style: TextStyle(
                    fontSize: FontSizes.small3.size,
                    color: colors.text.tertiary,
                    height: 1.0,
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// A "+N" chip standing in for reactions that didn't fit a narrow bubble.
/// Tapping it opens the who-reacted view on the "All" tab.
class _OverflowChip extends StatelessWidget {
  const _OverflowChip({
    required this.count,
    required this.isSender,
    this.onTap,
  });

  final int count;
  final bool isSender;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final borderColor = isSender
        ? colors.message.selfBackground
        : colors.message.otherBackground;
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: Container(
        height: reactionChipHeight,
        padding: const EdgeInsets.symmetric(
          horizontal: Spacing.px8,
          vertical: Spacing.px4,
        ),
        decoration: BoxDecoration(
          color: colors.backgroundElevated.primary,
          borderRadius: BorderRadius.circular(reactionChipHeight / 2),
          border: Border.all(color: borderColor),
        ),
        alignment: Alignment.center,
        child: Text(
          '+$count',
          style: TextStyle(
            fontSize: FontSizes.small2.size,
            color: colors.text.secondary,
            height: 1.0,
          ),
        ),
      ),
    );
  }
}

const Size _whoReactedPanelSize = Size(360, 380);
const double _reactorRowHeight = 56;
const double _reactorAvatarSize = 36;
const double _reactionTabHeight = 36;

/// Shows the "who reacted" view: a tabbed list (All + per emoji) of the users
/// who reacted to a message, with a "Remove" action on the current user's own
/// reactions.
Future<void> showWhoReactedSheet({
  required BuildContext context,
  required List<UiReaction> reactions,
  required UiUserId ownUserId,
  String? initialEmoji,
  required void Function(String emoji) onRemove,
}) {
  final platform = Theme.of(context).platform;
  final isMobile =
      platform == TargetPlatform.android || platform == TargetPlatform.iOS;
  final usersCubit = context.read<UsersCubit>();
  final profiles = <UiUserId, UiUserProfile>{};
  for (final reaction in reactions) {
    for (final user in reaction.users) {
      profiles[user] ??= usersCubit.state.profile(userId: user);
    }
  }
  final sheet = WhoReactedSheet(
    reactions: reactions,
    profiles: profiles,
    ownUserId: ownUserId,
    initialEmoji: initialEmoji,
    onRemove: onRemove,
  );
  if (isMobile) {
    return showBottomSheetModal<void>(
      context: context,
      builder: (context) => Padding(
        padding: const EdgeInsets.all(Spacing.px16),
        child: SizedBox(height: _whoReactedPanelSize.height, child: sheet),
      ),
    );
  }
  return showGeneralDialog<void>(
    context: context,
    barrierDismissible: true,
    barrierColor: const Color(0x33000000),
    barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    transitionDuration: const Duration(milliseconds: 150),
    pageBuilder: (context, animation, secondaryAnimation) =>
        const SizedBox.shrink(),
    transitionBuilder: (dialogContext, animation, secondaryAnimation, child) {
      final colors = CustomColorScheme.of(dialogContext);
      return FadeTransition(
        opacity: animation,
        child: Center(
          child: Material(
            type: MaterialType.transparency,
            child: Container(
              width: _whoReactedPanelSize.width,
              height: _whoReactedPanelSize.height,
              padding: const EdgeInsets.all(Spacing.px16),
              decoration: BoxDecoration(
                color: colors.backgroundElevated.primary,
                borderRadius: BorderRadius.circular(Spacing.px20),
                boxShadow: const [
                  BoxShadow(
                    color: Color(0x33000000),
                    blurRadius: 24,
                    offset: Offset(0, 8),
                  ),
                ],
              ),
              child: sheet,
            ),
          ),
        ),
      );
    },
  );
}

/// List all user profiles and their reactions in a "All" and single
/// tab per reaction.
class WhoReactedSheet extends StatefulWidget {
  const WhoReactedSheet({
    super.key,
    required this.reactions,
    required this.profiles,
    required this.ownUserId,
    this.initialEmoji,
    required this.onRemove,
  });

  final List<UiReaction> reactions;
  final Map<UiUserId, UiUserProfile> profiles;
  final UiUserId ownUserId;
  final String? initialEmoji;
  final void Function(String emoji) onRemove;

  @override
  State<WhoReactedSheet> createState() => _WhoReactedSheetState();
}

class _WhoReactedSheetState extends State<WhoReactedSheet> {
  // Mutable working copy so removals update the sheet optimistically.
  late final List<({String emoji, List<UiUserId> users})> _reactions = [
    for (final reaction in widget.reactions)
      (emoji: reaction.emoji, users: List<UiUserId>.of(reaction.users)),
  ];
  late String? _selected = widget.initialEmoji;

  int get _total =>
      _reactions.fold(0, (sum, reaction) => sum + reaction.users.length);

  List<({UiUserId user, String emoji})> get _rows {
    final rows = <({UiUserId user, String emoji})>[];
    for (final reaction in _reactions) {
      if (_selected != null && reaction.emoji != _selected) {
        continue;
      }
      for (final user in reaction.users) {
        rows.add((user: user, emoji: reaction.emoji));
      }
    }
    return rows;
  }

  void _remove(String emoji) {
    widget.onRemove(emoji);
    setState(() {
      for (final reaction in _reactions) {
        if (reaction.emoji == emoji) {
          reaction.users.remove(widget.ownUserId);
        }
      }
      _reactions.removeWhere((reaction) => reaction.users.isEmpty);
      if (_selected != null &&
          !_reactions.any((reaction) => reaction.emoji == _selected)) {
        _selected = null;
      }
    });

    Navigator.of(context).maybePop();
  }

  @override
  Widget build(BuildContext context) {
    final rows = _rows;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: _reactionTabHeight,
          child: ListView(
            scrollDirection: Axis.horizontal,
            children: [
              _ReactionTab(
                label: 'All · $_total',
                selected: _selected == null,
                onTap: () => setState(() => _selected = null),
              ),
              for (final reaction in _reactions)
                _ReactionTab(
                  label: '${reaction.emoji} ${reaction.users.length}',
                  selected: _selected == reaction.emoji,
                  onTap: () => setState(() => _selected = reaction.emoji),
                ),
            ],
          ),
        ),
        const SizedBox(height: Spacing.px12),
        Expanded(
          child: ListView.builder(
            padding: EdgeInsets.zero,
            itemCount: rows.length,
            // separatorBuilder: (context, index) =>
            // Divider(height: 1, color: colors.backgroundBase.tertiary),
            itemBuilder: (context, index) {
              final row = rows[index];
              final isMe = row.user == widget.ownUserId;
              final profile = widget.profiles[row.user];
              return _ReactorRow(
                profile: profile,
                name: isMe ? 'You' : (profile?.displayName ?? ''),
                emoji: row.emoji,
                onRemove: isMe ? () => _remove(row.emoji) : null,
              );
            },
          ),
        ),
      ],
    );
  }
}

class _ReactionTab extends StatelessWidget {
  const _ReactionTab({
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Padding(
      padding: const EdgeInsets.only(right: Spacing.px8),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Container(
          alignment: Alignment.center,
          padding: const EdgeInsets.symmetric(horizontal: Spacing.px12),
          decoration: BoxDecoration(
            color: selected
                ? colors.backgroundBase.secondary
                : Colors.transparent,
            borderRadius: BorderRadius.circular(_reactionTabHeight / 2),
          ),
          child: Text(
            label,
            style: TextStyle(
              fontSize: FontSizes.small2.size,
              color: selected ? colors.text.primary : colors.text.secondary,
            ),
          ),
        ),
      ),
    );
  }
}

class _ReactorRow extends StatelessWidget {
  const _ReactorRow({
    required this.profile,
    required this.name,
    required this.emoji,
    this.onRemove,
  });

  final UiUserProfile? profile;
  final String name;
  final String emoji;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return SizedBox(
      height: _reactorRowHeight,
      child: Row(
        children: [
          if (profile != null)
            UserAvatar(profile: profile!, size: _reactorAvatarSize)
          else
            const SizedBox(
              width: _reactorAvatarSize,
              height: _reactorAvatarSize,
            ),
          const SizedBox(width: Spacing.px12),
          Expanded(
            child: Text(
              name,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: FontSizes.base.size,
                color: colors.text.primary,
              ),
            ),
          ),
          if (onRemove != null) ...[
            MouseRegion(
              cursor: SystemMouseCursors.click,
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: onRemove,
                // TODO(l10n): localize "Remove".
                child: Text(
                  'Remove',
                  style: TextStyle(
                    fontSize: FontSizes.base.size,
                    color: colors.function.danger,
                  ),
                ),
              ),
            ),
            const SizedBox(width: Spacing.px12),
          ],
          Text(emoji, style: TextStyle(fontSize: FontSizes.base.size)),
        ],
      ),
    );
  }
}
