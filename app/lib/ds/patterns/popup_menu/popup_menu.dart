// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/components/menu/menu_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu_tokens.dart';
import 'package:flutter/material.dart'
    show Material, MaterialLocalizations, MaterialType;
import 'package:flutter/widgets.dart';

/// Which corner of a floating menu meets its anchor, and so which way the menu
/// opens out of it.
enum MenuCorner {
  /// Below the anchor, leading edges aligned.
  topLeft,

  /// Below the anchor, trailing edges aligned.
  topRight,

  /// Above the anchor, leading edges aligned.
  bottomLeft,

  /// Above the anchor, trailing edges aligned.
  bottomRight;

  bool get opensDown => this == topLeft || this == topRight;
  bool get alignsLeft => this == topLeft || this == bottomLeft;
}

/// Floats a [Menu] over the app, anchored to [anchor].
///
/// [anchor] is the trigger's bounds in global coordinates -- a zero-size rect
/// for a pointer position. [corner] picks the corner of the menu that meets it,
/// and so the direction the menu opens. The menu flips to the anchor's other
/// side when the preferred one is short of room, and never crosses into the
/// safe area.
///
/// The menu closes on a tap outside it, on Escape, and on selecting an item.
/// An item's `onPressed` runs after the menu is gone, and the returned future
/// completes once it is.
Future<void> showOverlayMenu({
  required BuildContext context,
  required Rect anchor,
  required List<MenuItem> items,
  MenuCorner corner = MenuCorner.topLeft,
  MenuTokens? tokens,
  double? slideDistance,
}) {
  final distance = slideDistance ?? PopupMenuTokens.slideDistance;
  // The menu starts offset toward the anchor and closes the gap, so it reads as
  // coming out of its trigger.
  final travel = corner.opensDown ? -distance : distance;
  final curve = CurveTween(curve: Effect.easeOutQuart);

  return showGeneralDialog<void>(
    context: context,
    barrierDismissible: true,
    barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    // No scrim: the barrier is here to catch the dismissing tap, not to dim the
    // app behind a menu.
    barrierColor: const Color(0x00000000),
    transitionDuration: Effect.duration(PopupMenuTokens.enter),
    transitionBuilder: (context, animation, _, child) => FadeTransition(
      opacity: animation.drive(curve),
      child: _SlideIn(
        distance: animation.drive(
          Tween<double>(begin: travel, end: 0).chain(curve),
        ),
        child: child,
      ),
    ),
    pageBuilder: (context, _, _) => _OverlayMenuPage(
      anchor: anchor,
      corner: corner,
      items: items,
      tokens: tokens ?? MenuTokens.current,
    ),
  );
}

/// Shifts its child along the vertical axis by an absolute distance, where
/// `SlideTransition` moves it by a fraction of its own size.
class _SlideIn extends StatelessWidget {
  const _SlideIn({required this.distance, required this.child});

  final Animation<double> distance;
  final Widget child;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: distance,
    child: child,
    builder: (context, child) =>
        Transform.translate(offset: Offset(0, distance.value), child: child),
  );
}

class _OverlayMenuPage extends StatelessWidget {
  const _OverlayMenuPage({
    required this.anchor,
    required this.corner,
    required this.items,
    required this.tokens,
  });

  final Rect anchor;
  final MenuCorner corner;
  final List<MenuItem> items;
  final MenuTokens tokens;

  @override
  Widget build(BuildContext context) {
    final media = MediaQuery.of(context);
    // Either inset can be the live one -- a notch on one edge, the keyboard on
    // another -- so each side takes whichever is larger.
    final inset =
        EdgeInsets.fromLTRB(
          math.max(media.viewPadding.left, media.viewInsets.left),
          math.max(media.viewPadding.top, media.viewInsets.top),
          math.max(media.viewPadding.right, media.viewInsets.right),
          math.max(media.viewPadding.bottom, media.viewInsets.bottom),
        ) +
        const EdgeInsets.all(PopupMenuTokens.edgeInset);

    final local = _toOverlaySpace(context, anchor);

    return Material(
      type: .transparency,
      child: LayoutBuilder(
        builder: (context, constraints) => CustomSingleChildLayout(
          delegate: _PopupMenuLayout(
            anchor: local,
            corner: corner,
            gap: PopupMenuTokens.anchorGap,
            inset: inset,
          ),
          child: Menu(
            tokens: tokens,
            items: [for (final item in items) _dismissing(context, item)],
            maxHeight: math.max(0.0, constraints.maxHeight - inset.vertical),
          ),
        ),
      ),
    );
  }

  /// The anchor arrives in global coordinates while the menu lays out inside
  /// the overlay, which the app's interface scale can move and scale.
  Rect _toOverlaySpace(BuildContext context, Rect rect) {
    final overlay = Overlay.of(context).context.findRenderObject();
    if (overlay is! RenderBox || !overlay.hasSize) return rect;
    return Rect.fromPoints(
      overlay.globalToLocal(rect.topLeft),
      overlay.globalToLocal(rect.bottomRight),
    );
  }

  /// Every item takes the menu down before it runs, so its action lands on an
  /// unobstructed screen and a dialog it opens isn't stacked under a menu.
  MenuItem _dismissing(BuildContext context, MenuItem item) {
    if (item.hasSubmenu) {
      return item.copyWith(
        subItems: [for (final sub in item.subItems) _dismissing(context, sub)],
      );
    }
    if (item.onPressed case final onPressed?) {
      return item.copyWith(
        onPressed: () {
          Navigator.of(context).pop();
          onPressed();
        },
      );
    }
    return item;
  }
}

/// Places a menu at [anchor], on the side [corner] asks for where there's room
/// for it and on the opposite side where there isn't, clamped inside [inset].
class _PopupMenuLayout extends SingleChildLayoutDelegate {
  const _PopupMenuLayout({
    required this.anchor,
    required this.corner,
    required this.gap,
    required this.inset,
  });

  final Rect anchor;
  final MenuCorner corner;
  final double gap;
  final EdgeInsets inset;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      BoxConstraints.loose(
        Size(
          math.max(0.0, constraints.maxWidth - inset.horizontal),
          math.max(0.0, constraints.maxHeight - inset.vertical),
        ),
      );

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final minX = inset.left;
    final maxX = math.max(minX, size.width - inset.right - childSize.width);
    final minY = inset.top;
    final maxY = math.max(minY, size.height - inset.bottom - childSize.height);

    final left = anchor.left;
    final right = anchor.right - childSize.width;
    final below = anchor.bottom + gap;
    final above = anchor.top - gap - childSize.height;

    final dx = corner.alignsLeft
        ? (left <= maxX ? left : right)
        : (right >= minX ? right : left);
    final dy = corner.opensDown
        ? (below <= maxY ? below : above)
        : (above >= minY ? above : below);

    return Offset(dx.clamp(minX, maxX), dy.clamp(minY, maxY));
  }

  @override
  bool shouldRelayout(_PopupMenuLayout oldDelegate) =>
      oldDelegate.anchor != anchor ||
      oldDelegate.corner != corner ||
      oldDelegate.gap != gap ||
      oldDelegate.inset != inset;
}
