// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/components/scroll/edge_fade.dart';
import 'package:air/ds/components/scroll/scroll_edges.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:flutter/material.dart' show Material, MaterialType;
import 'package:flutter/widgets.dart';

/// The surface a modal's content sits on: a centered card over the layout
/// where there's room beside it, the whole screen where there isn't.
///
/// Pure renderer. The scrim and the barrier belong to the route, so this
/// neither paints over nor intercepts anything behind it.
class ModalShell extends StatelessWidget {
  const ModalShell({super.key, required this.tokens, required this.child});

  final ModalShellTokens tokens;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final surface = ModalShellTokens.surface(context);

    // The same signal that picked the token set, so the presentation and its
    // geometry can't disagree.
    if (context.breakpoint.isSmall) {
      // Full-bleed: the modal is the screen, so it owns the status bar inset.
      // viewPadding rather than padding, so the value survives an ancestor
      // having already consumed the inset.
      return ColoredBox(
        color: surface,
        child: Padding(
          padding: EdgeInsets.only(top: MediaQuery.viewPaddingOf(context).top),
          child: child,
        ),
      );
    }

    return Padding(
      padding: tokens.containerPadding,
      child: Center(
        child: Container(
          // Hugs its content, clamped to the envelope.
          constraints: BoxConstraints(
            minWidth: tokens.minWidth,
            maxWidth: tokens.maxWidth,
            minHeight: tokens.minHeight,
            maxHeight: tokens.maxHeight,
          ),
          decoration: BoxDecoration(
            color: surface,
            borderRadius: BorderRadius.circular(tokens.cardRadius),
          ),
          clipBehavior: Clip.antiAlias,
          child: child,
        ),
      ),
    );
  }
}

/// The modal's chrome row: a title centered in the row between a leading and a
/// trailing action. The title centers on the row rather than on the space
/// the actions leave over, so it stays put as they come and go, and it gives
/// way to them once it's too long to fit between them.
///
/// Opaque, so it occludes content scrolling under the row.
class DialogHeader extends StatelessWidget {
  const DialogHeader({
    super.key,
    required this.tokens,
    required this.title,
    this.onLeading,
    this.onTrailing,
    this.trailing,
    this.leadingIcon = AppIconType.arrowLeft,
    this.trailingIcon = AppIconType.x,
  }) : assert(
         onTrailing == null || trailing == null,
         'DialogHeader takes either onTrailing or trailing, not both',
       );

  final DialogHeaderTokens tokens;
  final String title;

  /// Tap handler for the leading action. The button renders only when set.
  final VoidCallback? onLeading;

  /// Tap handler for the trailing action. The icon button renders only when
  /// set. Mutually exclusive with [trailing].
  final VoidCallback? onTrailing;

  /// Action rendered in the trailing slot in place of the icon button, for a
  /// header whose action carries a label. Mutually exclusive with
  /// [onTrailing].
  final Widget? trailing;

  final AppIconType leadingIcon;
  final AppIconType trailingIcon;

  /// Clearance between the title and either action, so a title that runs long
  /// ellipsizes short of them instead of touching them.
  static const double _titleGap = S.s8;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    // A floor, not a cap: it reserves the room a lone icon needs to balance
    // the row, while an action that wants more takes it rather than squashing.
    final slotWidth = math.max(tokens.slotWidth, tokens.actionSize);

    final trailingAction =
        trailing ??
        (onTrailing != null
            ? DialogHeaderAction(
                tokens: tokens,
                icon: trailingIcon,
                onPressed: onTrailing,
              )
            : null);

    return ColoredBox(
      // The shell's own resolver picks this fill, not code here, so the row
      // can never sit on one that differs from the card it caps.
      color: ModalShellTokens.surface(context),
      child: SizedBox(
        height: tokens.height,
        child: Padding(
          padding: tokens.contentPadding,
          // A toolbar rather than a row of fixed slots: it measures both
          // actions before it lays the title out, which is what lets the
          // trailing slot grow to a label while the title stays centered.
          child: NavigationToolbar(
            middleSpacing: _titleGap,
            leading: onLeading != null
                ? _Slot(
                    width: slotWidth,
                    alignment: Alignment.centerLeft,
                    child: DialogHeaderAction(
                      tokens: tokens,
                      icon: leadingIcon,
                      onPressed: onLeading,
                    ),
                  )
                : null,
            middle: Text(
              title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: typeScale.body.regular.style(
                color: palette.text.primary,
                weight: Weight.emphasized,
                tight: true,
              ),
            ),
            trailing: trailingAction != null
                ? _Slot(
                    width: slotWidth,
                    alignment: Alignment.centerRight,
                    child: trailingAction,
                  )
                : null,
          ),
        ),
      ),
    );
  }
}

/// The shape every modal in the app takes: a [DialogHeader] pinned above its
/// content, both inside a [ModalShell].
///
/// Composition only, so a feature supplies a title, its actions, and a body,
/// and never re-derives which token set each part takes. The header is opaque
/// and the content passes under it, so a fade bedded just below the row keeps
/// the seam soft instead of letting text cut off at a hard edge. It shows only
/// once there's content under the row: at rest the first line sits on the fill
/// the fade paints, so fading it there washes it out and buries the scrollbar.
class ModalScaffold extends StatefulWidget {
  const ModalScaffold({
    super.key,
    required this.title,
    this.onLeading,
    this.onTrailing,
    this.trailing,
    this.scrollable = true,
    required this.child,
  }) : assert(
         onTrailing == null || trailing == null,
         'ModalScaffold takes either onTrailing or trailing, not both',
       );

  final String title;

  /// Tap handler for the header's leading action. The button renders only when
  /// set.
  final VoidCallback? onLeading;

  /// Tap handler for the header's trailing action. The icon button renders only
  /// when set. Mutually exclusive with [trailing].
  final VoidCallback? onTrailing;

  /// Action for the header's trailing slot, in place of the icon button.
  /// Mutually exclusive with [onTrailing].
  final Widget? trailing;

  /// Whether the pattern scrolls [child] for it.
  ///
  /// A body that scrolls something of its own takes `false`: we then hand it
  /// the height the header leaves over as a tight constraint, so its own
  /// `Expanded` resolves and its list scrolls inside the card. The card fills
  /// its height envelope in that mode instead of hugging the content. Only the
  /// body then knows where its scroll region starts, so the fade and the
  /// scrollbar below are the body's to place.
  final bool scrollable;

  /// The modal's content. Where [scrollable], it scrolls as one block: a modal
  /// body is short enough that laziness buys nothing and costs the card its
  /// intrinsic height.
  final Widget child;

  /// Depth of the fade bedding the header. Deep enough to read as a ramp, short
  /// enough to leave the first row of content legible.
  static const double _fadeHeight = S.s24;

  @override
  State<ModalScaffold> createState() => _ModalScaffoldState();
}

class _ModalScaffoldState extends State<ModalScaffold> {
  /// We hold it in a notifier rather than in state so a scroll repaints the
  /// fade alone and never rebuilds the body behind it.
  final _edges = ValueNotifier<ScrollEdges>(ScrollEdges.atRest);

  @override
  void dispose() {
    _edges.dispose();
    super.dispose();
  }

  bool _track(int depth, ScrollMetrics metrics) {
    // A scrollable nested inside a row reports through here too, at a depth
    // below the one this fade stands for.
    if (depth == 0 && metrics.axis == Axis.vertical) {
      _edges.value = ScrollEdges.of(metrics);
    }
    return false;
  }

  @override
  Widget build(BuildContext context) {
    return ModalShell(
      tokens: ModalShellTokens.of(context),
      // A modal replaces the scaffold that used to host these bodies, so it has
      // to keep supplying the ink surface any Material descendant looks for.
      // Transparent, so the shell's own fill is what shows.
      child: Material(
        type: MaterialType.transparency,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            DialogHeader(
              tokens: DialogHeaderTokens.of(context),
              title: widget.title,
              onLeading: widget.onLeading,
              onTrailing: widget.onTrailing,
              trailing: widget.trailing,
            ),
            // Flexible keeps a scrolling body hugging its content, so the card
            // still sizes to it. Expanded hands a self-scrolling one the rest
            // of the envelope, which is the bounded height its own list needs.
            if (widget.scrollable)
              Flexible(child: _scrollingBody(context))
            else
              Expanded(child: widget.child),
          ],
        ),
      ),
    );
  }

  /// The body the pattern scrolls itself, bedded under the header by a fade and
  /// marked by a scrollbar of its own.
  ///
  /// Both sit inside the scroll area rather than over the whole card, so their
  /// geometry is the viewport's: the fade starts where the content does, and
  /// the thumb runs the height the content scrolls through. The scrollbar wraps
  /// the fade, so the thumb stays legible where the two cross.
  Widget _scrollingBody(BuildContext context) {
    return AppScrollbar(
      child: Stack(
        children: [
          ScrollConfiguration(
            // The scrollbar above is ours. The platform's would double it up.
            behavior: ScrollConfiguration.of(
              context,
            ).copyWith(scrollbars: false),
            // Metrics notifications cover what a scroll can't: the first
            // layout, and a body that grows or shrinks while it sits still.
            child: NotificationListener<ScrollMetricsNotification>(
              onNotification: (notification) =>
                  _track(notification.depth, notification.metrics),
              child: NotificationListener<ScrollNotification>(
                onNotification: (notification) =>
                    _track(notification.depth, notification.metrics),
                child: SingleChildScrollView(child: widget.child),
              ),
            ),
          ),
          Positioned(
            top: 0,
            left: 0,
            right: 0,
            child: EdgeFadeReveal(
              edges: _edges,
              edge: FadeEdge.top,
              child: EdgeFade(
                edge: FadeEdge.top,
                height: ModalScaffold._fadeHeight,
                color: ModalShellTokens.surface(context),
                curve: Curves.easeInOutQuad,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// One of the header's two edge slots. Hugs its action, down to a floor of
/// [width] so a lone icon still reserves the room the opposite slot would take.
class _Slot extends StatelessWidget {
  const _Slot({
    required this.width,
    required this.alignment,
    required this.child,
  });

  final double width;
  final Alignment alignment;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ConstrainedBox(
      constraints: BoxConstraints(minWidth: width),
      // widthFactor pins the slot to its action rather than letting it take
      // the row, which is what the toolbar measures to place the title. We
      // give the action loose constraints, so the floor above widens the slot
      // around it instead of stretching it.
      child: Align(
        alignment: alignment,
        widthFactor: 1.0,
        // A button handed a loose width fills it, so an action that carries a
        // label would take the row and leave the title nothing. Its intrinsic
        // width is the label's, which is the width the slot has to reserve.
        child: IntrinsicWidth(child: child),
      ),
    );
  }
}

/// An action in one of the header's two slots. Owned by the pattern so both
/// slots, and a host filling the trailing slot itself, carry the same button
/// treatment.
///
/// The row is opaque, so there's nothing behind the button for a frosted
/// material to resolve: it takes the same solid fill lifted by a flat shadow
/// that the list and chat headers give theirs.
class DialogHeaderAction extends StatelessWidget {
  const DialogHeaderAction({
    super.key,
    required this.tokens,
    required this.icon,
    this.onPressed,
  });

  final DialogHeaderTokens tokens;
  final AppIconType icon;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return ButtonIcon(
      variant: ButtonIconVariant.solid,
      size: tokens.actionSize,
      icon: icon,
      fill: SemanticPalette.of(context).backgroundElevated.primary,
      shadows: Effect.elevation(Elevation.flat),
      onPressed: onPressed,
    );
  }
}
