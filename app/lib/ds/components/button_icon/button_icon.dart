// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:ui' show BlurStyle, ImageFilter, MaskFilter;

import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/foundation.dart' show listEquals;
import 'package:flutter/widgets.dart';

/// Round icon button. Its look follows [variant], and its interaction
/// (hover / press / focus) rides on the shared [StateLayer], shaped by the
/// platform -- touch dips, a pointer lifts.
///
/// Without a handler the button reads as disabled: the glyph fades to
/// [StateTokens.disabledContent] while the fill only recedes to
/// [StateTokens.disabledFill] and the shadow stays, so the button keeps its
/// shape rather than dissolving into what it sits on.
class ButtonIcon extends StatelessWidget {
  const ButtonIcon({
    super.key,
    required this.variant,
    this.icon,
    this.iconWidget,
    this.size = ButtonIconSize.s40,
    this.iconSize,
    this.iconColor,
    this.fill,
    this.shadows,
    this.hitTargetSize,
    this.enableBackdropBlur = true,
    this.onPressed,
    this.onLongPress,
  }) : assert(
         icon != null || iconWidget != null,
         'ButtonIcon needs a glyph: pass icon or iconWidget',
       );

  final ButtonIconVariant variant;

  /// Glyph drawn in the button, sized and colored by the button.
  final AppIconType? icon;

  /// Glyph that is not an [AppIcon] (an emoji, say). Takes precedence over
  /// [icon] and owns its own metrics.
  final Widget? iconWidget;

  /// Button diameter. Off-scale values are for buttons that have to line up
  /// with adjacent text, see [ButtonIconSize].
  final double size;

  /// Glyph size. Defaults to the pair for [size].
  final double? iconSize;

  /// Glyph color. Defaults to `text.primary`.
  final Color? iconColor;

  /// Fill override, for a button that has to carry the color of what it sits
  /// on (a message bubble, a selected cell) rather than its variant's.
  final Color? fill;

  /// Shadow override. Pass `const []` where an ancestor already lifts the
  /// region, or an [Effect.elevation] tier to lift a solid button.
  final List<BoxShadow>? shadows;

  /// Outer hit target. Defaults to [size]. Pass a larger value to give a small
  /// button a generous tap area without enlarging the visible circle.
  final double? hitTargetSize;

  /// Whether an elevated button renders its own [BackdropFilter]. Disable
  /// where an ancestor already blurs the region (the message composer), as
  /// blurring twice costs a second pass and reads heavier.
  final bool enableBackdropBlur;

  final VoidCallback? onPressed;
  final VoidCallback? onLongPress;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final enabled = onPressed != null || onLongPress != null;

    // Fade the parts rather than wrapping the button in an Opacity: an opacity
    // layer over a BackdropFilter doesn't survive every renderer.
    final glyphFade = enabled ? 1.0 : StateTokens.disabledContent;
    final fillFade = enabled ? 1.0 : StateTokens.disabledFill;
    final baseFill = fill ?? ButtonIconTokens.fill(palette, variant);
    final bg = baseFill.withValues(alpha: baseFill.a * fillFade);
    final boxShadow = shadows ?? ButtonIconTokens.shadows(variant);
    final glyphColor = iconColor ?? palette.text.primary;

    final glyph =
        iconWidget ??
        AppIcon(
          type: icon!,
          size: iconSize ?? ButtonIconSize.glyphFor(size),
          color: glyphColor.withValues(alpha: glyphColor.a * glyphFade),
        );

    Widget circle = SizedBox.square(
      dimension: size,
      child: StateLayer(
        borderRadius: size / 2,
        surface: bg,
        enabled: enabled,
        onTap: onPressed,
        onLongPress: onLongPress,
        hoverScale: true,
        background: _Surface(
          variant: variant,
          fill: bg,
          shadows: boxShadow,
          enableBackdropBlur: enableBackdropBlur,
        ),
        child: Center(child: glyph),
      ),
    );

    final hit = hitTargetSize;
    if (hit != null && hit > size) {
      // The visible circle keeps the interaction states, so StateLayer stays
      // wrapped around it and the ring around it only has to carry the tap.
      // Nested detectors resolve innermost-first, so a tap on the circle
      // reaches the handler once.
      circle = GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onPressed,
        onLongPress: onLongPress,
        child: SizedBox.square(
          dimension: hit,
          child: Center(child: circle),
        ),
      );
    }

    return circle;
  }
}

/// The button's fill. Elevated is the frosted material -- a backdrop blur with
/// the translucent tint composited on top, lifted by a knockout shadow. Every
/// other variant is a flat circle.
class _Surface extends StatelessWidget {
  const _Surface({
    required this.variant,
    required this.fill,
    required this.shadows,
    required this.enableBackdropBlur,
  });

  final ButtonIconVariant variant;
  final Color fill;
  final List<BoxShadow> shadows;
  final bool enableBackdropBlur;

  @override
  Widget build(BuildContext context) {
    if (variant != ButtonIconVariant.elevated) {
      // An opaque fill hides whatever the shadow paints underneath it, so an
      // ordinary shadow already reads as if it were knocked out.
      return DecoratedBox(
        decoration: BoxDecoration(
          color: fill,
          shape: BoxShape.circle,
          boxShadow: shadows,
        ),
      );
    }

    final circleFill = DecoratedBox(
      decoration: BoxDecoration(color: fill, shape: BoxShape.circle),
    );

    final frost = enableBackdropBlur
        ? ClipOval(
            child: BackdropFilter(
              filter: ImageFilter.blur(
                sigmaX: ButtonIconTokens.elevatedBlur,
                sigmaY: ButtonIconTokens.elevatedBlur,
              ),
              child: circleFill,
            ),
          )
        : circleFill;

    if (shadows.isEmpty) return frost;
    return CustomPaint(painter: _KnockoutShadowPainter(shadows), child: frost);
  }
}

/// Paints [shadows] around a circle with the circle's own footprint knocked
/// out of them: the shadows go into an isolated layer, then we clear the
/// circle from that layer with [BlendMode.dstOut]. What survives casts only
/// outside the circle, so the translucent frosted fill reveals the blurred
/// content behind the button rather than the button's own shadow.
class _KnockoutShadowPainter extends CustomPainter {
  const _KnockoutShadowPainter(this.shadows);

  final List<BoxShadow> shadows;

  @override
  void paint(Canvas canvas, Size size) {
    final center = size.center(Offset.zero);
    final radius = size.shortestSide / 2;

    // The layer has to cover the furthest any shadow reaches, or the blur
    // clips at its own bounds.
    var reach = 0.0;
    for (final shadow in shadows) {
      final extent =
          shadow.blurRadius + shadow.spreadRadius + shadow.offset.distance;
      if (extent > reach) reach = extent;
    }

    canvas.saveLayer(
      Rect.fromCircle(center: center, radius: radius + reach + 1),
      Paint(),
    );
    for (final shadow in shadows) {
      canvas.drawCircle(
        center + shadow.offset,
        radius + shadow.spreadRadius,
        Paint()
          ..color = shadow.color
          ..maskFilter = MaskFilter.blur(BlurStyle.normal, shadow.blurSigma),
      );
    }
    canvas.drawCircle(center, radius, Paint()..blendMode = BlendMode.dstOut);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_KnockoutShadowPainter oldDelegate) =>
      !listEquals(oldDelegate.shadows, shadows);
}
