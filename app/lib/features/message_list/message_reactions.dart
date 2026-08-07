// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/components/emoji/centered_emoji.dart';
import 'package:air/ds/components/reaction_chip/reaction_chip.dart';
import 'package:air/ds/components/reaction_chip/reaction_chip_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/patterns/reaction_bar/reaction_bar.dart';
import 'package:air/ds/patterns/reaction_bar/reaction_bar_tokens.dart';
import 'package:air/ds/patterns/reaction_details/reaction_details.dart';
import 'package:air/ds/patterns/reaction_details/reaction_details_tokens.dart';
import 'package:air/ds/patterns/reaction_strip/reaction_strip.dart';
import 'package:air/ds/patterns/reaction_strip/reaction_strip_tokens.dart';
import 'package:air/features/emoji/emoji_repository.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/platform/haptics.dart';
import 'package:air/util/cached_memory_image.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// The curated quick-reaction set shown in the reaction bar.
const List<({String emoji, bool skinnable})> quickReactionEmojis = [
  (emoji: '👍', skinnable: true),
  (emoji: '❤️', skinnable: false),
  (emoji: '😂', skinnable: false),
  (emoji: '😮', skinnable: false),
  (emoji: '😢', skinnable: false),
  (emoji: '🙏', skinnable: true),
];

/// Size of the reactor panel on desktop, where it opens as a dialog rather
/// than as a sheet the platform sizes.
const Size _reactorPanelSize = Size(360, 380);

/// The quick-reaction set ready to send: [tone] applied to the emojis that
/// take one, the rest left alone.
List<String> quickReactionEmojisFor(EmojiSkinVariation tone) => [
  for (final item in quickReactionEmojis) _applyQuickTone(item, tone),
];

String _applyQuickTone(
  ({String emoji, bool skinnable}) item,
  EmojiSkinVariation tone,
) {
  if (!item.skinnable || tone == EmojiSkinVariation.none) {
    return item.emoji;
  }
  return '${item.emoji}${tone.modifier}';
}

/// Pre-measures the ink corrections for the quick-reaction emojis at chip
/// size, so the chips of common reactions render centered on first frame
/// instead of snapping into place (see [CenteredEmoji]). Chips for other
/// emojis measure lazily, once per process.
void warmUpReactionEmojis(BuildContext context) {
  CenteredEmoji.warmUp(context, [
    for (final item in quickReactionEmojis) item.emoji,
  ], ReactionChip.glyphStyle());
}

/// Vertical space [BubbleWithReactions] reserves below the bubble for the
/// chips that ride up over its bottom edge.
///
/// The chip grows with the text scaler, so the reserve grows with it too.
double reactionsReservedBelow(BuildContext context, bool hasReactions) {
  if (!hasReactions) return 0;
  final tokens = ReactionChipTokens.current;
  return MediaQuery.textScalerOf(context).scale(tokens.minHeight) +
      ReactionChipTokens.cropWidth * 2;
}

/// Overlays a [ReactionStrip] onto the bottom edge of [bubble].
///
/// The layout reserves the strip's height below the bubble so following
/// messages don't collide, and the strip lifts itself back up over the
/// bubble's edge.
///
/// The reserve and the chips animate in when the first reaction arrives and
/// out when the last one is removed. Tiles that mount with reactions render
/// the settled state without animating.
class BubbleWithReactions extends StatefulWidget {
  const BubbleWithReactions({
    super.key,
    required this.bubble,
    required this.reactions,
    required this.ownUserId,
    required this.onTap,
  });

  final Widget bubble;
  final List<UiReaction> reactions;
  final UiUserId ownUserId;

  /// Reveals who reacted. Null stands for the collapsed `+N` chip, which
  /// belongs to no single emoji.
  final void Function(String? emoji) onTap;

  @override
  State<BubbleWithReactions> createState() => _BubbleWithReactionsState();
}

class _BubbleWithReactionsState extends State<BubbleWithReactions>
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
      // Mirrored so removal collapses the reserve in sync with the
      // AnimatedPaddings tracking it in the message tile.
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
  void didUpdateWidget(covariant BubbleWithReactions oldWidget) {
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
    final reserve = reactionsReservedBelow(context, true);

    // Constant Stack/Padding structure so the first reaction doesn't
    // reparent the bubble subtree (which would drop its state).
    return Stack(
      clipBehavior: Clip.none,
      children: [
        AnimatedBuilder(
          animation: _reveal,
          builder: (context, child) => Padding(
            padding: EdgeInsets.only(bottom: reserve * _reveal.value),
            child: child,
          ),
          child: widget.bubble,
        ),
        if (_reactions.isNotEmpty)
          // Stretched across the bubble's own box, so the strip packs to the
          // bubble's width and starts at its leading edge.
          Positioned(
            left: 0,
            right: 0,
            bottom: 0,
            child: IgnorePointer(
              ignoring: widget.reactions.isEmpty,
              child: FadeTransition(
                opacity: _reveal,
                child: ScaleTransition(
                  scale: _chipScale,
                  alignment: Alignment.bottomLeft,
                  child: ReactionStrip(
                    tokens: ReactionStripTokens.current,
                    chipTokens: ReactionChipTokens.current,
                    groups: [
                      for (final reaction in _reactions)
                        ReactionGroup(
                          emoji: reaction.emoji,
                          count: reaction.users.length,
                          mine: reaction.users.contains(widget.ownUserId),
                        ),
                    ],
                    onTapEmoji: widget.onTap,
                    onTapOverflow: () => widget.onTap(null),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

/// Shows the [ReactionBar] as a small popover centered horizontally on
/// [anchorRect] and placed just above it, falling back to below it when there
/// isn't room above.
///
/// [onMore] opens the full picker on top of this dialog (with a transparent
/// barrier, see `openFullEmojiPicker`). This dialog stays alive underneath so
/// the barrier dim doesn't flicker, and pops once the picker resolves.
Future<void> showQuickReactionMenu({
  required BuildContext context,
  required Rect anchorRect,
  required EmojiSkinVariation skinTone,
  required void Function(String emoji) onReact,
  required Future<void> Function() onMore,
}) {
  // Once the full picker has been opened the bar stays hidden for good,
  // otherwise it would fade back in while this route pops (the picker's
  // dismissal reverses [secondaryAnimation] concurrently with the pop).
  var handedOff = false;
  // Drop taps that land during the closing transition.
  var consumed = false;
  return showGeneralDialog(
    context: context,
    barrierDismissible: true,
    barrierColor: SemanticPalette.of(context).function.neutral.scrim,
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
      // Own the focus and Escape handling explicitly. When opened from the
      // context menu, closing that menu tears down its focus node and reverts
      // focus to the page underneath.
      return Focus(
        autofocus: true,
        onKeyEvent: (node, event) {
          if (event is KeyDownEvent &&
              event.logicalKey == LogicalKeyboardKey.escape) {
            if (!consumed) {
              consumed = true;
              Navigator.of(dialogContext).maybePop();
            }
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        // The bar fades out while the full picker covers this route.
        child: FadeTransition(
          opacity: handedOff
              ? const AlwaysStoppedAnimation(Alpha.a0)
              : ReverseAnimation(secondaryAnimation),
          // Dialog routes live in the navigator's overlay, above the page's
          // Material
          child: Material(
            type: MaterialType.transparency,
            child: _QuickReactionMenuOverlay(
              animation: curved,
              anchorRect: anchorRect,
              skinTone: skinTone,
              onReact: (emoji) {
                if (consumed) return;
                consumed = true;
                Navigator.of(dialogContext).pop();
                onReact(emoji);
              },
              onMore: () {
                if (consumed) return;
                consumed = true;
                handedOff = true;
                unawaited(
                  onMore().whenComplete(() {
                    if (dialogContext.mounted) {
                      Navigator.of(dialogContext).pop();
                    }
                  }),
                );
              },
            ),
          ),
        ),
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
    const inset = ReactionBarTokens.screenInset;
    final safeArea = EdgeInsets.only(
      top: mediaQuery.padding.top + inset,
      bottom: mediaQuery.padding.bottom + inset,
      left: inset,
      right: inset,
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
      delegate: ReactionBarAnchorLayout(anchorRect: anchor, safeArea: safeArea),
      child: FadeTransition(
        opacity: animation,
        child: ScaleTransition(
          scale: Tween<double>(begin: 0.92, end: 1.0).animate(animation),
          alignment: Alignment.bottomCenter,
          child: ReactionBar(
            tokens: ReactionBarTokens.current,
            emojis: quickReactionEmojisFor(skinTone),
            onPick: onReact,
            onMore: onMore,
          ),
        ),
      ),
    );
  }
}

/// Shows the "who reacted" viewer: a tabbed list (All + one tab per emoji) of
/// the users who reacted to a message, with a "Remove" action on the current
/// user's own reactions.
Future<void> showWhoReactedSheet({
  required BuildContext context,
  required List<UiReaction> reactions,
  required UiUserId ownUserId,
  String? initialEmoji,
  required void Function(String emoji) onRemove,
}) {
  final loc = AppLocalizations.of(context);
  final platform = Theme.of(context).platform;
  final isMobile =
      platform == TargetPlatform.android || platform == TargetPlatform.iOS;
  final tokens = ReactionDetailsTokens.current;
  final entries = _reactorEntries(
    context,
    reactions: reactions,
    ownUserId: ownUserId,
    avatarSize: tokens.avatarSize,
    youLabel: loc.messageList_reactions_you,
  );

  final barrierColor = SemanticPalette.of(context).function.neutral.scrim;

  Widget viewer(BuildContext viewerContext) => ReactionDetails(
    tokens: tokens,
    entries: entries,
    allLabel: loc.messageList_reactions_all(entries.length),
    removeLabel: loc.messageList_reactions_remove,
    initialEmoji: initialEmoji,
    onRemove: (emoji) {
      AppHaptics.selection();
      onRemove(emoji);
      Navigator.of(viewerContext).maybePop();
    },
  );

  if (isMobile) {
    return showAdaptiveModal<void>(
      context: context,
      barrierColor: barrierColor,
      builder: (modalContext) => SizedBox(
        height: _reactorPanelSize.height,
        child: viewer(modalContext),
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
      final palette = SemanticPalette.of(dialogContext);
      return FadeTransition(
        opacity: animation,
        child: Center(
          child: Material(
            type: MaterialType.transparency,
            child: Container(
              width: _reactorPanelSize.width,
              height: _reactorPanelSize.height,
              padding: const EdgeInsets.symmetric(vertical: S.s16),
              decoration: BoxDecoration(
                color: palette.backgroundElevated.primary,
                borderRadius: BorderRadius.circular(CornerRadius.px20),
                boxShadow: Effect.elevation(Elevation.small),
              ),
              child: viewer(dialogContext),
            ),
          ),
        ),
      );
    },
  );
}

/// One entry per (emoji, reactor), with the profile resolved and the picture
/// decoded to the size the viewer paints it at.
List<ReactionDetailEntry> _reactorEntries(
  BuildContext context, {
  required List<UiReaction> reactions,
  required UiUserId ownUserId,
  required double avatarSize,
  required String youLabel,
}) {
  final usersCubit = context.read<UsersCubit>();
  final pixels = (avatarSize * MediaQuery.devicePixelRatioOf(context)).round();

  final entries = <ReactionDetailEntry>[];
  for (final reaction in reactions) {
    for (final user in reaction.users) {
      final profile = usersCubit.state.profile(userId: user);
      final picture = profile.profilePicture;
      final mine = user == ownUserId;
      entries.add(
        ReactionDetailEntry(
          displayName: mine ? youLabel : profile.displayName,
          emoji: reaction.emoji,
          image: picture != null
              ? CachedMemoryImage.fromImageData(
                  picture,
                  targetWidth: pixels,
                  targetHeight: pixels,
                )
              : null,
          gradientSeed: profile.userId.uuid.uuid,
          mine: mine,
        ),
      );
    }
  }
  return entries;
}
