// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/material.dart';

typedef SuggestionOverlayItemBuilder<T> =
    Widget Function(BuildContext context, T item, bool isHighlighted);

class SuggestionOverlayStyle {
  const SuggestionOverlayStyle({
    required this.backgroundColor,
    required this.borderRadius,
    required this.elevation,
    required this.maxWidth,
  });

  final Color backgroundColor;
  final BorderRadius borderRadius;
  final double elevation;
  final double maxWidth;
}

enum SuggestionOverlayAnimationPhase { none, entering, exiting }

class SuggestionOverlayController<T> {
  /// Manages an anchored suggestion overlay tied to a specific text field.
  SuggestionOverlayController({
    required TickerProvider vsync,
    required this.anchorLink, // Link that positions the overlay relative to the field.
    required this.focusNode, // Focus node that drives keyboard interactions.
  }) : _animationController = AnimationController(
         vsync: vsync,
         duration: const Duration(milliseconds: 100),
       );

  final LayerLink anchorLink;
  final FocusNode focusNode;
  final AnimationController _animationController;

  OverlayEntry? _overlayEntry; // Active overlay entry attached to the Overlay.
  Offset _offset = Offset.zero; // Anchor offset relative to the caret.
  List<T> _suggestions = const []; // Current suggestion list.
  int _highlightIndex = 0; // Keyboard highlight index within suggestions.
  bool _visible = false; // Tracks whether the overlay should be shown.
  bool _pointerDown = false; // Indicates if the pointer is pressed on overlay.
  SuggestionOverlayStyle? _style; // Cached style applied to the overlay.
  SuggestionOverlayItemBuilder<T>? _itemBuilder; // Builder for suggestion rows.
  ValueChanged<T>? _onSelected; // Callback invoked when a suggestion is picked.
  Size _overlaySize = const Size(320, 280); // Last measured overlay size.
  final GlobalKey _overlayKey = GlobalKey(); // Key used for size measurements.
  SuggestionOverlayAnimationPhase _animationPhase =
      SuggestionOverlayAnimationPhase.none; // Current animation state.
  ValueChanged<Size>? _onSizeChanged; // Listener notified when overlay resizes.

  bool get isVisible => _visible && _suggestions.isNotEmpty;
  bool get isPointerDown => _pointerDown;
  Size get overlaySize => _overlaySize;
  List<T> get suggestions => _suggestions;
  int get highlightIndex => _highlightIndex;
  Offset get offset => _offset;
  SuggestionOverlayStyle? get style => _style;
  SuggestionOverlayItemBuilder<T>? get itemBuilder => _itemBuilder;
  AnimationController get animationController => _animationController;
  SuggestionOverlayAnimationPhase get animationPhase => _animationPhase;
  GlobalKey get overlayKey => _overlayKey;

  set onSizeChanged(ValueChanged<Size>? listener) {
    _onSizeChanged = listener;
  }

  /// Insert or refresh the overlay with the provided suggestions and builder.
  Future<void> show({
    required BuildContext context,
    required Offset offset,
    required List<T> suggestions,
    required SuggestionOverlayStyle style,
    required SuggestionOverlayItemBuilder<T> itemBuilder,
    required ValueChanged<T> onSelected,
  }) async {
    if (suggestions.isEmpty) {
      await dismiss();
      return;
    }
    final overlayState = Overlay.maybeOf(context);
    if (overlayState == null) {
      return;
    }
    final wasVisible = isVisible;
    _style = style;
    _itemBuilder = itemBuilder;
    _onSelected = onSelected;
    _offset = offset;
    _suggestions = suggestions;
    _highlightIndex = 0;
    if (_overlayEntry == null) {
      _overlayEntry = OverlayEntry(
        builder: (context) => _SuggestionOverlay<T>(controller: this),
      );
      overlayState.insert(_overlayEntry!);
    } else {
      _markNeedsBuild();
    }
    _visible = true;
    if (!wasVisible) {
      await _playAnimation(SuggestionOverlayAnimationPhase.entering);
    } else {
      _animationPhase = SuggestionOverlayAnimationPhase.entering;
      _animationController.value = 1;
    }
    _scheduleSizeUpdate();
  }

  /// Update the overlay anchor offset without rebuilding tiles.
  void updateOffset(Offset offset) {
    _offset = offset;
    _markNeedsBuild();
  }

  /// Move the keyboard highlight, wrapping the list length.
  void moveHighlight(int delta) {
    if (!isVisible || _suggestions.isEmpty) {
      return;
    }
    final length = _suggestions.length;
    var newIndex = _highlightIndex + delta;
    newIndex %= length;
    if (newIndex < 0) {
      newIndex += length;
    }
    _highlightIndex = newIndex;
    _markNeedsBuild();
  }

  /// Select the currently highlighted suggestion, if any.
  bool selectHighlighted() {
    if (!isVisible || _suggestions.isEmpty) {
      return false;
    }
    final index = _highlightIndex;
    if (index < 0 || index >= _suggestions.length) {
      return false;
    }
    _onSelected?.call(_suggestions[index]);
    return true;
  }

  /// Select a suggestion by index (used by pointer taps).
  void selectSuggestion(int index) {
    if (index < 0 || index >= _suggestions.length) {
      return;
    }
    _onSelected?.call(_suggestions[index]);
  }

  /// Remove the overlay entry, optionally playing the exit animation.
  Future<void> dismiss({bool animated = true}) async {
    if (_overlayEntry == null) {
      _visible = false;
      return;
    }
    if (animated && isVisible) {
      await _playAnimation(SuggestionOverlayAnimationPhase.exiting);
    }
    _overlayEntry?.remove();
    _overlayEntry = null;
    _visible = false;
    _animationPhase = SuggestionOverlayAnimationPhase.none;
    _suggestions = const [];
    _itemBuilder = null;
    _onSelected = null;
    _pointerDown = false;
  }

  /// Record that the pointer is pressed inside the overlay.
  void handlePointerDown() {
    _pointerDown = true;
  }

  /// Release pointer capture and ensure focus returns to the text field.
  void handlePointerUp() {
    _pointerDown = false;
    if (!focusNode.hasFocus) {
      focusNode.requestFocus();
    }
  }

  /// Reset pointer state when gestures are cancelled.
  void handlePointerCancel() {
    _pointerDown = false;
  }

  /// Remove any active overlay entry and dispose the animation controller.
  void dispose() {
    _overlayEntry?.remove();
    _animationController.dispose();
  }

  /// Measure the rendered overlay so caret offsets stay accurate.
  void updateOverlaySize() {
    final renderBox =
        overlayKey.currentContext?.findRenderObject() as RenderBox?;
    if (renderBox == null) {
      return;
    }
    final newSize = renderBox.size;
    if (newSize == _overlaySize) {
      return;
    }
    _overlaySize = newSize;
    _onSizeChanged?.call(newSize);
  }

  /// Schedule a size measurement after the current frame.
  void _scheduleSizeUpdate() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      updateOverlaySize();
    });
  }

  /// Drive the shared animation controller for entry or exit phases.
  Future<void> _playAnimation(SuggestionOverlayAnimationPhase phase) async {
    _animationPhase = phase;
    _animationController.duration = const Duration(milliseconds: 100);
    _markNeedsBuild();
    try {
      await _animationController.forward(from: 0);
    } catch (_) {}
  }

  void _markNeedsBuild() {
    _overlayEntry?.markNeedsBuild();
  }
}

class _SuggestionOverlay<T> extends StatelessWidget {
  const _SuggestionOverlay({required this.controller});

  final SuggestionOverlayController<T> controller;

  @override
  Widget build(BuildContext context) {
    if (!controller.isVisible ||
        controller.style == null ||
        controller.itemBuilder == null) {
      return const SizedBox.shrink();
    }
    final style = controller.style!;
    return Positioned(
      left: 0,
      top: 0,
      child: CompositedTransformFollower(
        link: controller.anchorLink,
        showWhenUnlinked: false,
        offset: controller.offset,
        child: AnimatedBuilder(
          animation: controller.animationController,
          builder: (context, child) {
            if (controller.animationPhase ==
                SuggestionOverlayAnimationPhase.none) {
              return const SizedBox.shrink();
            }
            final curve = CurvedAnimation(
              parent: controller.animationController,
              curve:
                  controller.animationPhase ==
                      SuggestionOverlayAnimationPhase.entering
                  ? Curves.easeInOutCubic
                  : Curves.easeInOutCubic,
            );
            final fadeAnimation =
                controller.animationPhase ==
                    SuggestionOverlayAnimationPhase.entering
                ? curve
                : Tween<double>(begin: 1, end: 0).animate(curve);
            final slideAnimation =
                controller.animationPhase ==
                    SuggestionOverlayAnimationPhase.entering
                ? Tween<Offset>(
                    begin: const Offset(0, 0.02),
                    end: Offset.zero,
                  ).animate(curve)
                : Tween<Offset>(
                    begin: Offset.zero,
                    end: const Offset(0, 0.02),
                  ).animate(curve);

            return FadeTransition(
              opacity: fadeAnimation,
              child: SlideTransition(position: slideAnimation, child: child),
            );
          },
          child: _SuggestionOverlayBody<T>(
            controller: controller,
            style: style,
          ),
        ),
      ),
    );
  }
}

class _SuggestionOverlayBody<T> extends StatelessWidget {
  const _SuggestionOverlayBody({required this.controller, required this.style});

  final SuggestionOverlayController<T> controller;
  final SuggestionOverlayStyle style;

  @override
  Widget build(BuildContext context) {
    final itemBuilder = controller.itemBuilder!;
    final suggestions = controller.suggestions;
    return Listener(
      onPointerDown: (_) => controller.handlePointerDown(),
      onPointerUp: (_) => controller.handlePointerUp(),
      onPointerCancel: (_) => controller.handlePointerCancel(),
      child: Material(
        key: controller.overlayKey,
        elevation: style.elevation,
        color: style.backgroundColor,
        borderRadius: style.borderRadius,
        clipBehavior: Clip.antiAlias,
        child: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: style.maxWidth),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: List.generate(suggestions.length, (index) {
              final item = suggestions[index];
              final isHighlighted = index == controller.highlightIndex;
              return InkWell(
                onTap: () => controller.selectSuggestion(index),
                child: itemBuilder(context, item, isHighlighted),
              );
            }),
          ),
        ),
      ),
    );
  }
}
