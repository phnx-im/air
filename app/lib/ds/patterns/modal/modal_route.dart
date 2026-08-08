// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:flutter/material.dart';

/// Presents [builder] the way a modal takes here: pushed as a screen where it
/// fills the viewport, floated as a card where it doesn't.
Future<T?> showAppModal<T>({
  required BuildContext context,
  required WidgetBuilder builder,
}) {
  final navigator = Navigator.of(context, rootNavigator: true);

  if (ModalShellTokens.isFullBleed(context)) {
    return navigator.push(MaterialPageRoute<T>(builder: builder));
  }
  return navigator.push(
    ModalCardRoute<T>(
      builder: builder,
      barrierColor: SemanticPalette.of(context).function.neutral.scrim,
    ),
  );
}

/// The card presentation for desktop.
class ModalCardRoute<T> extends PageRoute<T> {
  ModalCardRoute({
    required this.builder,
    required this.barrierColor,
    super.settings,
  });

  final WidgetBuilder builder;

  @override
  final Color barrierColor;

  @override
  String? get barrierLabel => null;

  @override
  bool get opaque => false;

  @override
  bool get barrierDismissible => true;

  @override
  bool get maintainState => true;

  /// The scrim
  @override
  Widget buildModalBarrier() => AnimatedModalBarrier(
    color: animation!.drive(
      ColorTween(
        begin: barrierColor.withValues(alpha: 0),
        end: barrierColor,
      ).chain(CurveTween(curve: barrierCurve)),
    ),
    onDismiss: () {},
    barrierSemanticsDismissible: false,
  );

  @override
  Duration get transitionDuration => Effect.duration(MotionPreset.regular);

  @override
  Duration get reverseTransitionDuration => Effect.duration(MotionPreset.short);

  @override
  Widget buildPage(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
  ) => builder(context);

  @override
  Widget buildTransitions(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) {
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
          child: Transform.scale(
            scale: from + (1.0 - from) * t,
            // Anchoring the card at the top of the screen
            alignment: Alignment.topCenter,
            child: child,
          ),
        );
      },
    );
  }
}
