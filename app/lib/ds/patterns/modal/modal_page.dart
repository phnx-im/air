// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:flutter/material.dart';

/// A modal in the router's page stack, presented per breakpoint: a card
/// floating over the two-pane layout where there's room beside it, an
/// ordinary pushed screen where there isn't.
class ModalPage<T> extends Page<T> {
  const ModalPage({
    super.key,
    super.name,
    super.arguments,
    required this.child,
  });

  final Widget child;

  @override
  Route<T> createRoute(BuildContext context) {
    // We resolve this here rather than at construction: createRoute runs under
    // the navigator, so the route matches the layout the modal opens into.
    if (context.breakpoint.isSmall) {
      return MaterialPageRoute<T>(settings: this, builder: (context) => child);
    }
    return _ModalCardRoute<T>(
      page: this,
      barrierColor: SemanticPalette.of(context).function.neutral.scrim,
      barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    );
  }
}

/// The card presentation. Non-opaque by design: the two-pane layout underneath
/// stays mounted and visible through the scrim.
class _ModalCardRoute<T> extends PageRoute<T> {
  _ModalCardRoute({
    required ModalPage<T> page,
    required this.barrierColor,
    required this.barrierLabel,
  }) : super(settings: page);

  // We read this back off the settings rather than capture it, so an updated
  // page swaps its content in instead of leaving the route on the stale child.
  ModalPage<T> get _page => settings as ModalPage<T>;

  @override
  final Color? barrierColor;

  @override
  final String? barrierLabel;

  @override
  bool get opaque => false;

  /// A tap on the scrim calls `Navigator.maybePop`, which flows through the
  /// router's `onPopPage` into the navigation cubit.
  @override
  bool get barrierDismissible => true;

  @override
  bool get maintainState => true;

  @override
  Duration get transitionDuration => Effect.duration(MotionPreset.regular);

  @override
  Duration get reverseTransitionDuration => Effect.duration(MotionPreset.short);

  @override
  Widget buildPage(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
  ) => _page.child;

  @override
  Widget buildTransitions(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) {
    // We read this per frame rather than drive it through a Tween: the scale
    // the card travels from depends on which way the route is going, which a
    // Tween can't see. Driving off the route's own animation also avoids a
    // CurvedAnimation, which would need disposing.
    return AnimatedBuilder(
      animation: animation,
      child: child,
      builder: (context, child) {
        final t = Effect.easeOutQuart.transform(animation.value);
        final from = animation.status == AnimationStatus.reverse
            ? ModalShellTokens.exitScaleEnd
            : ModalShellTokens.entryScaleBegin;
        return Opacity(
          opacity: t,
          child: Transform.scale(scale: from + (1.0 - from) * t, child: child),
        );
      },
    );
  }
}
