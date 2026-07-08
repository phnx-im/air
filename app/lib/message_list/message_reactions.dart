// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/ds/foundations/elevation.dart';
import 'package:air/l10n/l10n.dart';
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
import 'package:flutter_hooks/flutter_hooks.dart';

import 'emoji_repository.dart';

/// The curated quick-reaction set shown in the [QuickReactionBar].
const List<({String emoji, bool skinnable})> quickReactionEmojis = [
  (emoji: '👍', skinnable: true),
  (emoji: '❤️', skinnable: false),
  (emoji: '😂', skinnable: false),
  (emoji: '😮', skinnable: false),
  (emoji: '😢', skinnable: false),
  (emoji: '🙏', skinnable: true),
];

/// Size of emojis in the reaction bar
const double quickReactionBarGlyphSize = 28;

/// Size of the tappable area in the reaction bar
const double quickReactionBarTapSize = 44;

/// Size of the more (+) button in the reaction bar
const double quickReactionBarMoreSize = 36;

/// Fixed height of the reaction bar
const double quickReactionBarHeight = quickReactionBarTapSize + Spacing.px8;

/// Extra vertical space between the hit point and where the reaction bar opens
const double quickReactionMenuGap = Spacing.px12;

/// Compact reaction chip metrics
const double reactionChipSpacing = Spacing.px4;

/// Fixed height of a reaction chip
const double reactionChipHeight = 36;

/// How far the chips overlap the bottom border of the message bubble
const double reactionsMessageBubbleOverlap = Spacing.px8;

/// Gap below the chips before the timestamp / next message.
const double reactionsGapBelow = Spacing.px8;

/// Aligning the chips
const double reactionsHorizontalInset = Spacing.px8;

/// Vertical space [BubbleWithReactions] reserves below the bubble for the chips
/// that overlap its bottom edge.
double reactionsReservedBelow(bool hasReactions) => hasReactions
    ? reactionChipHeight - reactionsMessageBubbleOverlap + reactionsGapBelow
    : 0;

/// Default size of the reactor panel.
const Size whoReactedPanelSize = Size(360, 380);
const double reactorRowHeight = 56;
const double reactorAvatarSize = 36;
const double reactionTabHeight = 36;

/// Overlays [MessageReactions] onto the bottom edge of [bubble].
///
/// The chips overlap the bubble's bottom by [reactionsMessageBubbleOverlap] and the layout
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
    required this.onTap,
  });

  final Widget bubble;
  final List<UiReaction> reactions;
  final bool isSender;
  final UiUserId ownUserId;
  final void Function(String? emoji) onTap;

  @override
  Widget build(BuildContext context) {
    if (reactions.isEmpty) {
      return bubble;
    }
    return Stack(
      clipBehavior: Clip.none,
      children: [
        Padding(
          // reactions is non-empty here, so reserve the chips' protrusion.
          padding: EdgeInsets.only(bottom: reactionsReservedBelow(true)),
          child: bubble,
        ),
        Positioned(
          bottom: reactionsGapBelow,
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
class MessageReactions extends StatelessWidget {
  const MessageReactions({
    super.key,
    required this.reactions,
    required this.isSender,
    required this.ownUserId,
    required this.onTap,
  });

  final List<UiReaction> reactions;
  final bool isSender;
  final UiUserId ownUserId;
  final void Function(String? emoji) onTap;

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
    final chipTextStyle = _ReactionChip.textStyle();

    double measure(String text, TextStyle style) {
      final painter = TextPainter(
        text: TextSpan(text: text, style: style),
        textDirection: TextDirection.ltr,
        textScaler: scaler,
      )..layout();
      return painter.width;
    }

    // Horizontal chrome of a chip: padding (both sides) + border (both sides).
    // Must match the padding used by _ReactionChip/_OverflowChip below.
    const chipChrome = Spacing.px12 * 2 + 2;
    double chipWidth(UiReaction reaction) {
      var width = chipChrome + measure(reaction.emoji, chipTextStyle);
      if (reaction.users.length >= 2) {
        width +=
            Spacing.px4 + measure('${reaction.users.length}', chipTextStyle);
      }
      return width;
    }

    Widget chipFor(UiReaction reaction, {int? extras}) => _ReactionChip(
      reaction: reaction,
      extras: extras,
      isSender: isSender,
      isMine: reaction.users.contains(ownUserId),
      onTap: () => onTap(reaction.emoji),
    );

    // Chip widths depend only on the reaction data and text scaler, not the
    // incoming constraints, so measure once rather than on every layout pass.
    final widths = [for (final reaction in ordered) chipWidth(reaction)];
    final count = ordered.length;

    return LayoutBuilder(
      builder: (context, constraints) {
        const fitSlack = 1.0;
        final maxWidth = constraints.maxWidth - fitSlack;

        var fullWidth = 0.0;
        for (var i = 0; i < count; i++) {
          fullWidth += widths[i] + (i > 0 ? reactionChipSpacing : 0);
        }

        final List<Widget> chips;
        if (!maxWidth.isFinite || fullWidth <= maxWidth) {
          chips = [for (final reaction in ordered) chipFor(reaction)];
        } else {
          // Reserve room for the overflow chip ("+N" upper bound).
          final overflowReserve =
              reactionChipSpacing +
              chipChrome +
              measure('+$count', chipTextStyle);
          var used = 0.0;
          var shown = 0;
          for (var i = 0; i < count; i++) {
            final add = widths[i] + (shown > 0 ? reactionChipSpacing : 0);
            if (used + add + overflowReserve <= maxWidth) {
              used += add;
              shown++;
            } else {
              break;
            }
          }
          if (shown == 0) {
            // Too narrow for even one emoji beside the "+N" chip: collapse
            // every reaction into a single overflow chip.
            chips = [
              _OverflowChip(
                count: count,
                isSender: isSender,
                onTap: () => onTap(null),
              ),
            ];
          } else {
            final overflow = count - shown;
            chips = [
              for (var i = 0; i < shown; i++)
                chipFor(ordered[i], extras: overflow),
              _OverflowChip(
                count: overflow,
                isSender: isSender,
                onTap: () => onTap(null),
              ),
            ];
          }
        }

        final children = <Widget>[];
        for (var i = 0; i < chips.length; i++) {
          if (i > 0) children.add(const SizedBox(width: reactionChipSpacing));
          children.add(chips[i]);
        }
        // Size to the chips' natural width rather than filling maxWidth: even
        // the single "+N" fallback chip can exceed maxWidth on a very narrow
        // bubble. OverflowBox lets it grow past the bubble edge instead of
        // force-fitting into maxWidth and triggering a RenderFlex overflow.
        return SizedBox(
          height: reactionChipHeight,
          child: OverflowBox(
            minWidth: 0,
            maxWidth: double.infinity,
            alignment: isSender ? Alignment.centerLeft : Alignment.centerRight,
            child: Row(mainAxisSize: MainAxisSize.min, children: children),
          ),
        );
      },
    );
  }
}

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
class QuickReactionBar extends StatelessWidget {
  const QuickReactionBar({
    super.key,
    required this.onReact,
    required this.onMore,
    this.skinTone = EmojiSkinVariation.none,
    this.showShadow = true,
  });

  final void Function(String emoji) onReact;
  final VoidCallback onMore;
  final EmojiSkinVariation skinTone;
  final bool showShadow;

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
        borderRadius: BorderRadius.circular(
          (quickReactionBarTapSize + Spacing.px8) / 2,
        ),
        boxShadow: mediumElevationBoxShadows,
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final emoji in quickReactionEmojis.map(
            (item) => _applyQuickTone(item, skinTone),
          ))
            _QuickReactionButton(emoji: emoji, onTap: () => onReact(emoji)),
          GlassCircleButton(
            onPressed: onMore,
            size: quickReactionBarMoreSize,
            hitTargetSize: quickReactionBarTapSize,
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

/// Shows the [QuickReactionBar] as a small popover centered horizontally on
/// [anchorRect] and placed just above it, falling back to below it when there
/// isn't room above.
Future<void> showQuickReactionMenu({
  required BuildContext context,
  required Rect anchorRect,
  required EmojiSkinVariation skinTone,
  required void Function(String emoji) onReact,
  required VoidCallback onMore,
}) {
  return showGeneralDialog(
    context: context,
    barrierDismissible: true,
    barrierColor: CustomColorScheme.of(context).function.barrier,
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

class _QuickReactionMenuOverlay extends StatelessWidget {
  const _QuickReactionMenuOverlay({
    required this.animation,
    required this.anchorRect,
    required this.skinTone,
    required this.onReact,
    required this.onMore,
  });

  final Animation<double> animation;
  final Rect anchorRect;
  final EmojiSkinVariation skinTone;
  final void Function(String emoji) onReact;
  final VoidCallback onMore;

  @override
  Widget build(BuildContext context) {
    final mediaQuery = MediaQuery.of(context);
    final safeArea = EdgeInsets.only(
      top: mediaQuery.padding.top + Spacing.px8,
      bottom: mediaQuery.padding.bottom + Spacing.px8,
      left: Spacing.px8,
      right: Spacing.px8,
    );

    final overlayBox =
        Overlay.of(context).context.findRenderObject() as RenderBox?;
    final anchor = overlayBox == null
        ? anchorRect
        : Rect.fromPoints(
            overlayBox.globalToLocal(anchorRect.topLeft),
            overlayBox.globalToLocal(anchorRect.bottomRight),
          );

    return CustomSingleChildLayout(
      delegate: _QuickReactionBarLayoutDelegate(
        anchorRect: anchor,
        safeArea: safeArea,
      ),
      child: FadeTransition(
        opacity: animation,
        child: ScaleTransition(
          scale: Tween<double>(begin: 0.92, end: 1.0).animate(animation),
          alignment: Alignment.bottomCenter,
          child: QuickReactionBar(
            onReact: onReact,
            onMore: onMore,
            skinTone: skinTone,
          ),
        ),
      ),
    );
  }
}

/// Centers the bar horizontally on the anchor and places it [quickReactionMenuGap]
/// above the anchor, flipping below when there isn't room above. Everything is
/// clamped into the safe area.
class _QuickReactionBarLayoutDelegate extends SingleChildLayoutDelegate {
  const _QuickReactionBarLayoutDelegate({
    required this.anchorRect,
    required this.safeArea,
  });

  final Rect anchorRect;
  final EdgeInsets safeArea;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) {
    // Let the bar size to its content instead of filling the dialog.
    return constraints.loosen();
  }

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    // Centered horizontally on the anchor.
    var dx = anchorRect.center.dx - childSize.width / 2;

    // Above the anchor if it fits, otherwise below it.
    final above = anchorRect.top - quickReactionMenuGap - childSize.height;
    final below = anchorRect.bottom + quickReactionMenuGap;
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
  bool shouldRelayout(_QuickReactionBarLayoutDelegate oldDelegate) =>
      oldDelegate.anchorRect != anchorRect || oldDelegate.safeArea != safeArea;
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
          width: quickReactionBarTapSize,
          height: quickReactionBarTapSize,
          child: Center(
            child: Text(
              emoji,
              style: const TextStyle(
                fontSize: quickReactionBarGlyphSize,
                decoration: TextDecoration.none,
                height: 1.0,
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
    this.extras,
  });

  final UiReaction reaction;
  final bool isSender;
  final bool isMine;
  final int? extras;
  final VoidCallback? onTap;

  static TextStyle textStyle({Color? color}) =>
      TextStyle(fontSize: BodyFontSize.base.size, height: 1.0, color: color);

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
          padding: const EdgeInsets.symmetric(horizontal: Spacing.px12),
          decoration: BoxDecoration(
            color: isMine
                ? colors.message.selfBackground
                : colors.message.otherBackground,
            borderRadius: BorderRadius.circular(reactionChipHeight / 2),
            border: Border.all(color: colors.backgroundBase.primary),
          ),
          child: Row(
            mainAxisSize: .min,
            crossAxisAlignment: .center,
            children: [
              Text(reaction.emoji, style: textStyle()),
              if (count >= 2) ...[
                const SizedBox(width: Spacing.px4),
                Text('$count', style: textStyle(color: colors.text.tertiary)),
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
    required this.onTap,
  });

  final int count;

  final bool isSender;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Container(
          height: reactionChipHeight,
          padding: const EdgeInsets.symmetric(
            horizontal: Spacing.px12,
            vertical: Spacing.px4,
          ),
          decoration: BoxDecoration(
            color: isSender
                ? colors.message.selfBackground
                : colors.message.otherBackground,
            borderRadius: BorderRadius.circular(reactionChipHeight / 2),
            border: Border.all(color: colors.backgroundBase.primary),
          ),
          alignment: Alignment.center,
          child: Text(
            '+$count',
            textAlign: TextAlign.center,
            style: _ReactionChip.textStyle(color: colors.text.tertiary),
          ),
        ),
      ),
    );
  }
}

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
  final barrierColor = CustomColorScheme.of(context).function.barrier;
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
      barrierColor: barrierColor,
      builder: (context) => Padding(
        padding: const EdgeInsets.all(Spacing.px16),
        child: SizedBox(height: whoReactedPanelSize.height, child: sheet),
      ),
    );
  }
  return showGeneralDialog<void>(
    context: context,
    barrierDismissible: true,
    barrierColor: barrierColor,
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
              width: whoReactedPanelSize.width,
              height: whoReactedPanelSize.height,
              padding: const EdgeInsets.all(Spacing.px16),
              decoration: BoxDecoration(
                color: colors.backgroundElevated.primary,
                borderRadius: BorderRadius.circular(Spacing.px20),
                boxShadow: smallElevationBoxShadows,
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
class WhoReactedSheet extends HookWidget {
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
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final selected = useState(initialEmoji);

    final total = reactions.fold<int>(
      0,
      (sum, reaction) => sum + reaction.users.length,
    );

    final rows = <({UiUserId user, String emoji})>[];
    for (final reaction in reactions) {
      if (selected.value != null && reaction.emoji != selected.value) {
        continue;
      }
      for (final user in reaction.users) {
        rows.add((user: user, emoji: reaction.emoji));
      }
    }

    void remove(String emoji) {
      onRemove(emoji);
      Navigator.of(context).maybePop();
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: reactionTabHeight,
          child: ListView(
            scrollDirection: Axis.horizontal,
            children: [
              _ReactionTab(
                label: loc.messageList_reactions_all(total),
                selected: selected.value == null,
                onTap: () => selected.value = null,
              ),
              for (final reaction in reactions)
                _ReactionTab(
                  label: '${reaction.emoji} ${reaction.users.length}',
                  selected: selected.value == reaction.emoji,
                  onTap: () => selected.value = reaction.emoji,
                ),
            ],
          ),
        ),
        const SizedBox(height: Spacing.px12),
        Expanded(
          child: ListView.builder(
            padding: EdgeInsets.zero,
            itemCount: rows.length,
            itemBuilder: (context, index) {
              final row = rows[index];
              final isMe = row.user == ownUserId;
              final profile = profiles[row.user];
              return _ReactorRow(
                profile: profile,
                name: isMe
                    ? loc.messageList_reactions_you
                    : (profile?.displayName ?? ''),
                emoji: row.emoji,
                onRemove: isMe ? () => remove(row.emoji) : null,
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
      padding: const EdgeInsets.only(right: Spacing.px4),
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
            borderRadius: BorderRadius.circular(reactionTabHeight / 2),
          ),
          child: Text(
            label,
            style: TextStyle(
              fontSize: BodyFontSize.large1.size,
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
    final loc = AppLocalizations.of(context);
    return SizedBox(
      height: reactorRowHeight,
      child: Row(
        children: [
          if (profile != null)
            UserAvatar(profile: profile!, size: reactorAvatarSize)
          else
            const SizedBox(width: reactorAvatarSize, height: reactorAvatarSize),
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
                child: Text(
                  loc.messageList_reactions_remove,
                  style: TextStyle(
                    fontSize: FontSizes.base.size,
                    color: colors.function.danger,
                  ),
                ),
              ),
            ),
            const SizedBox(width: Spacing.px12),
          ],
          Text(emoji, style: TextStyle(fontSize: BodyFontSize.large1.size)),
        ],
      ),
    );
  }
}
