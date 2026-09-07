// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// Whether a pointer hover lifts the surface, and on what terms.
enum HoverLift {
  /// The surface only takes the hover wash.
  none,

  /// The surface lifts whatever width it ends up at.
  always,

  /// The surface lifts only where it picked its own width. One stretched to a
  /// width it was handed grows into the margin around it rather than within
  /// its own footprint, which reads as the layout shifting under the pointer.
  selfSized,
}

/// One interaction layer every interactive component shares, so they all
/// hover, press and focus the same way:
///
///   * hover -- white wash at [StateTokens.hoverOverlay], plus an optional
///     pointer lift to [StateTokens.hoverScale].
///   * pressed -- black wash at [StateTokens.pressedOverlay]. Touch dips to
///     [StateTokens.pressedScale], a pointer settles back to 1.0.
///   * focused -- keyboard-only focus ring in `function.link` at
///     [StateTokens.focusRingWidth].
///
/// The wash flips direction to stay visible: a near-white surface can't get
/// brighter so it darkens on hover, a near-black one can't get darker so it
/// lightens on press. The host passes us the [surface] and we handle the
/// rest. The feedback shape ([hover] / [pressScale]) follows the device, a
/// host only names it to deviate.
class StateLayer extends StatefulWidget {
  const StateLayer({
    super.key,
    required this.borderRadius,
    required this.surface,
    required this.child,
    this.background,
    this.onTap,
    this.onLongPress,
    this.enabled = true,
    this.hover,
    this.selected = false,
    this.press = true,
    this.pressScale,
    this.hoverLift = HoverLift.none,
  });

  /// Corner radius of the host surface, so the wash and focus ring line up.
  final double borderRadius;

  /// The color the wash sits on: the component's own background, or the
  /// parent's if it's transparent. When it's translucent we blend it over the
  /// page base first, then read that luminance to pick the wash direction.
  final Color surface;

  final Widget child;

  /// Optional surface painted behind the wash and [child], so the wash tints
  /// the background but leaves text and glyphs crisp. Null for a transparent
  /// surface, where the wash just tints the footprint.
  final Widget? background;

  final VoidCallback? onTap;
  final VoidCallback? onLongPress;
  final bool enabled;

  /// Whether a pointer hover paints the hover wash. Defaults from the device,
  /// on for a pointer platform and off for touch. Pass a value only to
  /// deviate.
  final bool? hover;

  /// Whether this is the selected surface. It already has its own fill, so we
  /// skip the hover wash here or it'd double up. Press still fires.
  final bool selected;

  /// Whether a press does anything at all, wash and dip. Pass false when the
  /// host brings its own feedback, like the tab bar's sliding pill. Mirrors
  /// [hover].
  final bool press;

  /// Whether a press dips the surface by [StateTokens.pressedScale]. Defaults
  /// from the device, on for touch and off for a pointer platform. Pass a
  /// value only to deviate.
  final bool? pressScale;

  /// Whether a pointer hover lifts the surface to [StateTokens.hoverScale],
  /// and on what terms. The lift only applies while hovered, so it is inert
  /// on touch and a call site can name its terms unconditionally.
  final HoverLift hoverLift;

  @override
  State<StateLayer> createState() => _StateLayerState();
}

class _StateLayerState extends State<StateLayer> {
  bool _hovered = false;
  bool _pressed = false;
  bool _focused = false;

  void _setPressed(bool value) {
    if (widget.enabled && _pressed != value) {
      setState(() => _pressed = value);
    }
  }

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final duration = Effect.duration(MotionPreset.short);
    final radius = BorderRadius.circular(widget.borderRadius);

    // The platform default: a pointer hovers, touch dips on press.
    final hover = widget.hover ?? !DeviceType.isPhone;
    final pressScale = widget.pressScale ?? DeviceType.isPhone;

    // Only hover when the surface accepts it and isn't selected, otherwise a
    // just-selected item under the pointer would keep a stale wash.
    final hovered = widget.enabled && hover && !widget.selected && _hovered;
    final pressed = widget.enabled && widget.press && _pressed;

    // Blend the (maybe translucent) surface onto the page base before reading
    // luminance. Hover and press are separate layers that each animate only
    // their own alpha, so the color never lerps through some in-between hue.
    final white = palette.function.neutral.white;
    final black = palette.function.neutral.black;
    final luminance = Color.alphaBlend(
      widget.surface,
      palette.backgroundBase.primary,
    ).computeLuminance();
    final hoverDarkens = luminance >= StateTokens.lightSurfaceCeiling;
    final pressDarkens = luminance > StateTokens.darkSurfaceFloor;
    final hoverWash = hoverDarkens ? black : white;
    final pressWash = pressDarkens ? black : white;

    // The hover lift scales with luminance: light surfaces lift toward white,
    // dark or saturated ones only gently so they don't blow out. Near-white
    // darkens gently instead. Press always uses its full alpha.
    final hoverAlpha = (hovered && !pressed)
        ? (hoverDarkens
              ? StateTokens.hoverDarkenOverlay
              : StateTokens.hoverOverlay * luminance)
        : Alpha.a0;
    final pressAlpha = pressed ? StateTokens.pressedOverlay : Alpha.a0;

    // Touch dips on press, a pointer lifts on hover and settles back on click.
    final scale = pressed
        ? (pressScale ? StateTokens.pressedScale : 1.0)
        : (widget.hoverLift != HoverLift.none && hovered
              ? StateTokens.hoverScale
              : 1.0);

    // Only grab gestures when there's actually something to tap, so an inert
    // surface doesn't swallow a tap meant for a handler behind it.
    final interactive =
        widget.enabled && (widget.onTap != null || widget.onLongPress != null);

    return FocusableActionDetector(
      enabled: interactive,
      mouseCursor: interactive ? SystemMouseCursors.click : MouseCursor.defer,
      onShowHoverHighlight: (value) =>
          setState(() => _hovered = hover && value),
      onShowFocusHighlight: (value) => setState(() => _focused = value),
      child: GestureDetector(
        // Opaque when interactive so the whole footprint is tappable, not just
        // the child. Otherwise a nav item with a transparent child only reacts
        // right over its glyph and text.
        behavior: interactive ? .opaque : .deferToChild,
        onTapDown: interactive ? (_) => _setPressed(true) : null,
        onTapUp: interactive ? (_) => _setPressed(false) : null,
        onTapCancel: interactive ? () => _setPressed(false) : null,
        onTap: interactive ? widget.onTap : null,
        onLongPress: interactive ? widget.onLongPress : null,
        child: TweenAnimationBuilder<double>(
          tween: Tween<double>(begin: 1.0, end: scale),
          duration: duration,
          curve: Effect.easeOutQuart,
          builder: (context, value, child) => _LiftScale(
            scale: value,
            selfSizedOnly: widget.hoverLift == HoverLift.selfSized,
            child: child!,
          ),
          child: Stack(
            // Pass constraints straight through so a component still fills a
            // tight slot instead of shrinking down to its content width.
            fit: .passthrough,
            children: [
              // Background sits at the bottom and takes the wash. Content
              // paints on top, so it stays crisp.
              if (widget.background != null)
                Positioned.fill(child: widget.background!),
              Positioned.fill(
                child: IgnorePointer(
                  child: AnimatedContainer(
                    duration: duration,
                    curve: Effect.easeOutQuart,
                    decoration: BoxDecoration(
                      color: hoverWash.withValues(alpha: hoverAlpha),
                      borderRadius: radius,
                    ),
                  ),
                ),
              ),
              Positioned.fill(
                child: IgnorePointer(
                  child: AnimatedContainer(
                    duration: duration,
                    curve: Effect.easeOutQuart,
                    decoration: BoxDecoration(
                      color: pressWash.withValues(alpha: pressAlpha),
                      borderRadius: radius,
                    ),
                  ),
                ),
              ),
              widget.child,
              if (_focused)
                Positioned.fill(
                  child: IgnorePointer(
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        border: Border.all(
                          color: palette.function.link,
                          width: StateTokens.focusRingWidth,
                        ),
                        borderRadius: radius,
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Scales its child around its center the way `Transform.scale` does, except
/// that under [selfSizedOnly] a scale above 1.0 is dropped where the width
/// came from the parent.
class _LiftScale extends SingleChildRenderObjectWidget {
  const _LiftScale({
    required this.scale,
    required this.selfSizedOnly,
    required Widget super.child,
  });

  final double scale;
  final bool selfSizedOnly;

  @override
  _RenderLiftScale createRenderObject(BuildContext context) =>
      _RenderLiftScale(scale, selfSizedOnly);

  @override
  void updateRenderObject(BuildContext context, _RenderLiftScale renderObject) {
    renderObject
      ..scale = scale
      ..selfSizedOnly = selfSizedOnly;
  }
}

class _RenderLiftScale extends RenderProxyBox {
  _RenderLiftScale(this._scale, this._selfSizedOnly);

  double _scale;

  set scale(double value) {
    if (_scale == value) {
      return;
    }
    _scale = value;
    markNeedsPaint();
  }

  bool _selfSizedOnly;

  set selfSizedOnly(bool value) {
    if (_selfSizedOnly == value) {
      return;
    }
    _selfSizedOnly = value;
    markNeedsPaint();
  }

  /// The scale we actually paint. Only growth is held back: a dip lands inside
  /// the footprint wherever the width came from. Reading the value rather than
  /// the hover flag also keeps the settle-back from popping, since every frame
  /// on the way down is held at 1.0 too.
  double get _effective =>
      _scale > 1.0 && _selfSizedOnly && constraints.hasTightWidth
      ? 1.0
      : _scale;

  Matrix4 get _transform {
    final center = size.center(Offset.zero);
    final scale = _effective;
    return Matrix4.translationValues(center.dx, center.dy, 0.0)
      ..multiply(Matrix4.diagonal3Values(scale, scale, 1.0))
      ..multiply(Matrix4.translationValues(-center.dx, -center.dy, 0.0));
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    if (child == null) {
      return;
    }
    if (_effective == 1.0) {
      layer = null;
      super.paint(context, offset);
      return;
    }
    layer = context.pushTransform(
      needsCompositing,
      offset,
      _transform,
      super.paint,
      oldLayer: layer is TransformLayer ? layer as TransformLayer? : null,
    );
  }

  // A lift paints past the layout box, so the box check is skipped and the
  // child answers for the overhang, the same way `RenderTransform` does it.
  @override
  bool hitTest(BoxHitTestResult result, {required Offset position}) =>
      hitTestChildren(result, position: position);

  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) =>
      result.addWithPaintTransform(
        transform: _transform,
        position: position,
        hitTest: (result, position) =>
            super.hitTestChildren(result, position: position),
      );

  @override
  void applyPaintTransform(RenderBox child, Matrix4 transform) {
    transform.multiply(_transform);
  }
}
