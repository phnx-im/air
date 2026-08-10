// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:flutter/material.dart' show Material, MaterialType;
import 'package:flutter/widgets.dart';

/// One page of a [ModalPageStack].
@immutable
class ModalStackEntry {
  const ModalStackEntry({
    required this.key,
    this.canGoBack = true,
    this.canDismiss = true,
    required this.child,
  });

  /// Identifies the page across rebuilds, so it keeps what it holds
  final LocalKey key;

  /// Whether the page takes the stack's back action while it sits above the
  /// bottom. A page past a point of no return turns it off.
  final bool canGoBack;

  /// Whether the page takes the stack's dismiss.
  final bool canDismiss;

  /// The page itself, a [ModalPane].
  final Widget child;
}

/// A modal that pages: one surface, one scope of providers over it, and a page
/// per level of a drill-down.
class ModalPageStack extends StatefulWidget {
  const ModalPageStack({
    super.key,
    required this.pages,
    this.onBack,
    this.onDismiss,
  }) : assert(pages.length > 0, 'a paged modal shows at least one page');

  /// The stack, bottom first. The last entry is the page on top.
  final List<ModalStackEntry> pages;

  /// Drops the page on top. Every page above the bottom takes it as its back
  /// action, as does the system back gesture.
  final VoidCallback? onBack;

  /// Closes the modal, from any depth.
  final VoidCallback? onDismiss;

  @override
  State<ModalPageStack> createState() => _ModalPageStackState();
}

/// The page leaving the top, and which way the pages are travelling.
@immutable
class _Transition {
  const _Transition({
    required this.page,
    required this.canGoBack,
    required this.deeper,
  });

  final ModalStackEntry page;

  /// Whether the leaving page showed a back action while on top.
  final bool canGoBack;

  final bool deeper;
}

class _ModalPageStackState extends State<ModalPageStack>
    with SingleTickerProviderStateMixin {
  /// Matches the card's entrance, so drilling in reads at the same pace.
  static const _motion = MotionPreset.regular;

  /// How far a covered page travels, in fractions of its own width. Short of
  /// full width, so the pages read as a stack rather than a carousel.
  static const double _coveredTravel = -0.25;

  /// Built up front, because a stack that never pages still disposes one.
  late final AnimationController _controller;

  _Transition? _transition;

  /// One identity per page, so its element survives moving between layers
  final _identities = <LocalKey, GlobalKey>{};

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: Effect.duration(_motion),
    )..addStatusListener(_onTravelled);
  }

  @override
  void didUpdateWidget(ModalPageStack oldWidget) {
    super.didUpdateWidget(oldWidget);

    final leaving = oldWidget.pages.last;
    if (leaving.key == widget.pages.last.key) return;

    setState(() {
      _transition = _Transition(
        page: leaving,
        canGoBack: oldWidget.pages.length > 1 && leaving.canGoBack,
        deeper: widget.pages.length > oldWidget.pages.length,
      );
    });
    _controller.forward(from: 0);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _onTravelled(AnimationStatus status) {
    if (status != AnimationStatus.completed) return;
    setState(() => _transition = null);
  }

  @override
  Widget build(BuildContext context) {
    // Two levels sharing a key would be one page held in two places at once.
    assert(
      widget.pages.length == {for (final page in widget.pages) page.key}.length,
      'the pages of a modal stack take a key each',
    );

    final transition = _transition;
    _dropIdentitiesGoneFrom(transition);

    final top = widget.pages.last;
    final layers = <Widget>[];

    // What the page on top covers: in the tree, off the surface. Filled rather
    // than hugging, because a self-scrolling page needs a height even offstage.
    for (final (index, page) in widget.pages.indexed) {
      if (page.key == top.key || page.key == transition?.page.key) continue;
      layers.add(
        Positioned.fill(
          child: Offstage(child: _layer(page, canGoBack: index > 0)),
        ),
      );
    }

    // The only layer the surface measures, so it settles at the height of the
    // page arriving rather than of the tallest one on screen.
    final arriving = _travelling(
      from: transition == null ? 0 : (transition.deeper ? 1 : _coveredTravel),
      to: 0,
      child: _layer(top, canGoBack: widget.pages.length > 1),
    );

    if (transition == null) {
      layers.add(arriving);
    } else {
      final leaving = Positioned.fill(
        // A tap on a page on its way out would act on a level already left.
        child: IgnorePointer(
          child: _travelling(
            from: 0,
            to: transition.deeper ? _coveredTravel : 1,
            child: _layer(transition.page, canGoBack: transition.canGoBack),
          ),
        ),
      );
      // Going deeper, the page we came from passes under the one arriving.
      // Coming back, the page leaving passes over the one it uncovers.
      layers.addAll(
        transition.deeper ? [leaving, arriving] : [arriving, leaving],
      );
    }

    return PopScope(
      // Back goes up a level while there is one to go up to. Only the bottom
      // page lets the route holding the stack pop.
      canPop: widget.pages.length == 1 || widget.onBack == null,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) widget.onBack?.call();
      },
      child: ModalSurface(
        child: AnimatedSize(
          duration: Effect.duration(_motion),
          curve: Effect.easeOutQuart,
          // The header sits at the top, so height changes play out at the
          // bottom edge.
          alignment: Alignment.topCenter,
          child: Stack(alignment: Alignment.topCenter, children: layers),
        ),
      ),
    );
  }

  /// A page as one layer of the surface: opaque, so pages it passes never show
  /// through, and handed the actions its header takes.
  Widget _layer(ModalStackEntry page, {required bool canGoBack}) {
    return KeyedSubtree(
      key: _identityOf(page.key),
      child: Material(
        type: MaterialType.canvas,
        color: ModalShellTokens.surface(context),
        child: ModalPageActions(
          onBack: canGoBack && page.canGoBack ? widget.onBack : null,
          onDismiss: page.canDismiss ? widget.onDismiss : null,
          child: page.child,
        ),
      ),
    );
  }

  /// [child] on its way from [from] to [to], in fractions of its own width.
  Widget _travelling({
    required double from,
    required double to,
    required Widget child,
  }) {
    if (_transition == null) return child;
    return AnimatedBuilder(
      animation: _controller,
      child: child,
      builder: (context, child) {
        final t = Effect.easeOutQuart.transform(_controller.value);
        return FractionalTranslation(
          translation: Offset(from + (to - from) * t, 0),
          child: child,
        );
      },
    );
  }

  GlobalKey _identityOf(LocalKey key) =>
      _identities.putIfAbsent(key, GlobalKey.new);

  void _dropIdentitiesGoneFrom(_Transition? transition) {
    final live = <LocalKey>{for (final page in widget.pages) page.key};
    if (transition != null) live.add(transition.page.key);
    _identities.removeWhere((key, _) => !live.contains(key));
  }
}
