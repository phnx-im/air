// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/list_header/list_header_tokens.dart';
import 'package:flutter/widgets.dart';

/// The header of a list-depth screen: a centered title in a pill, with a
/// leading and a trailing slot. Transparent, so the list scrolls full-bleed
/// underneath and the frame's fade separates the two.
///
/// The pill only materializes once content slides under the bar, driven by
/// [scrollOffset]. The host owns the scroll, so this stays a pure renderer.
class ListHeader extends StatelessWidget {
  const ListHeader({
    super.key,
    required this.tokens,
    this.title,
    this.leading,
    this.trailing,
    this.scrollOffset = 0,
  });

  final ListHeaderTokens tokens;

  /// Rendered only where [ListHeaderTokens.showTitle].
  final String? title;

  /// Leading action, typically a [ListHeaderAction].
  final Widget? leading;

  final Widget? trailing;

  /// How far the content beneath the bar has scrolled, in pixels.
  final double scrollOffset;

  @override
  Widget build(BuildContext context) {
    final hasSlot = leading != null || trailing != null;

    // The slot width is a floor, not a cap: we don't squash an action larger
    // than it. Take the larger, and give it to both slots so the title stays
    // centered as the action grows.
    final slotWidth = math.max(tokens.slotSize, tokens.actionSize);

    return SizedBox(
      height: tokens.height,
      child: Padding(
        padding: EdgeInsets.only(
          left: tokens.paddingLeft,
          right: tokens.paddingRight,
        ),
        child: Row(
          children: [
            if (hasSlot)
              SizedBox(
                width: slotWidth,
                child: Align(alignment: Alignment.centerLeft, child: leading),
              ),
            Expanded(
              // Inset off the slots so the pill can never crowd the action.
              // This also bounds the pill's width, which is what makes an
              // over-long title ellipsize instead of pushing into the buttons.
              child: Padding(
                padding: EdgeInsets.symmetric(
                  horizontal: hasSlot ? tokens.titleGap : 0,
                ),
                child: Center(
                  child: (title != null && tokens.showTitle)
                      ? _Title(
                          title: title!,
                          tokens: tokens,
                          scrollOffset: scrollOffset,
                        )
                      : null,
                ),
              ),
            ),
            if (hasSlot)
              SizedBox(
                width: slotWidth,
                child: Align(alignment: Alignment.centerRight, child: trailing),
              ),
          ],
        ),
      ),
    );
  }
}

/// The header's leading action. Owned by the pattern so every list header
/// carries the same button treatment.
class ListHeaderAction extends StatelessWidget {
  const ListHeaderAction({
    super.key,
    required this.tokens,
    this.icon = AppIconType.squarePen,
    this.onAction,
  });

  final ListHeaderTokens tokens;
  final AppIconType icon;

  /// Handed the button's own context, so a menu opened from here anchors to
  /// the button rather than to whatever the host happens to build it under.
  final void Function(BuildContext buttonContext)? onAction;

  @override
  Widget build(BuildContext context) {
    final fill = SemanticPalette.of(context).backgroundElevated.primary;
    final onAction = this.onAction;

    // Builder so the context the handler receives resolves to the button and
    // not to this widget, which an ancestor may have padded or aligned.
    return Builder(
      builder: (buttonContext) => ButtonIcon(
        variant: ButtonIconVariant.solid,
        icon: icon,
        size: tokens.actionSize,
        fill: fill,
        shadows: Effect.elevation(Elevation.flat),
        onPressed: onAction == null ? null : () => onAction(buttonContext),
      ),
    );
  }
}

class _Title extends StatelessWidget {
  const _Title({
    required this.title,
    required this.tokens,
    required this.scrollOffset,
  });

  final String title;
  final ListHeaderTokens tokens;
  final double scrollOffset;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    // Scroll-linked reveal, mirroring the frame's top fade: ramp 0 to 1 over
    // the reveal distance, eased. It tracks the finger, so it needs no
    // duration.
    final t = (scrollOffset / ListHeaderTokens.pillRevealDistance).clamp(
      0.0,
      1.0,
    );
    final reveal = Effect.easeOutQuart.transform(t);
    final fill = palette.backgroundElevated.primary;
    final border = palette.separator.primary;

    return Container(
      constraints: BoxConstraints(minHeight: tokens.pillMinHeight),
      padding: tokens.pillPadding,
      decoration: BoxDecoration(
        color: fill.withValues(alpha: fill.a * reveal),
        borderRadius: BorderRadius.circular(ListHeaderTokens.pillRadius),
        // Outside-aligned so the stroke grows outward from the pill's edge
        // rather than inward: the label and its padding never shift as the
        // border comes in.
        border: ListHeaderTokens.pillBorderWidth > 0
            ? Border.all(
                color: border.withValues(alpha: border.a * reveal),
                width: ListHeaderTokens.pillBorderWidth,
                strokeAlign: BorderSide.strokeAlignOutside,
              )
            : null,
        boxShadow: [
          for (final shadow in ListHeaderTokens.pillShadow)
            shadow.copyWith(
              color: shadow.color.withValues(alpha: shadow.color.a * reveal),
            ),
        ],
      ),
      // No alignment here: a Container with one expands to its parent's
      // bounded width instead of hugging its label.
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Flexible(
            child: Text(
              title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              // Emphasized, like the modal header's: this label names the
              // screen, where the chat header's pill carries a person's name
              // and stays plain.
              style: typeScale.body.regular.style(
                color: palette.text.primary,
                weight: Weight.emphasized,
                tight: true,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
