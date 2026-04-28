// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:ui';

import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/effects/elevation.dart';
import 'package:air/ui/effects/material.dart';
import 'package:flutter/material.dart';

/// Circular glass button used for app bar chrome that floats over content
/// (back, plus, dismiss, composer actions).
class GlassCircleButton extends StatelessWidget {
  const GlassCircleButton({
    super.key,
    required this.icon,
    this.onPressed,
    this.onLongPress,
    this.size = 40,
    this.hitTargetSize,
    this.enableBackdropBlur = true,
    this.color,
    this.shadows = mediumElevationBoxShadows,
  });

  final Widget icon;
  final VoidCallback? onPressed;
  final VoidCallback? onLongPress;
  final double size;

  /// Outer hit target. Defaults to [size]; pass a larger value to give
  /// generous tap area without enlarging the visible circle.
  final double? hitTargetSize;

  /// Render an own [BackdropFilter] under the fill. Disable when an ancestor
  /// already blurs the region (e.g. the message composer).
  final bool enableBackdropBlur;

  /// Fill color. Defaults to `material.tertiary` for the translucent glass
  /// look. Pass an opaque color for non-glass surfaces.
  final Color? color;

  /// Drop shadows. Pass an empty list when an ancestor paints shadows for
  /// the surrounding element (e.g. the message composer).
  final List<BoxShadow> shadows;

  @override
  Widget build(BuildContext context) {
    final fillColor = color ?? CustomColorScheme.of(context).material.tertiary;
    final hitSize = hitTargetSize ?? size;
    final enabled = onPressed != null || onLongPress != null;

    final fill = DecoratedBox(
      decoration: BoxDecoration(color: fillColor, shape: BoxShape.circle),
      child: Center(child: icon),
    );

    Widget circle = SizedBox(
      width: size,
      height: size,
      child: DecoratedBox(
        decoration: BoxDecoration(shape: BoxShape.circle, boxShadow: shadows),
        child: enableBackdropBlur
            ? ClipOval(
                child: BackdropFilter(
                  filter: ImageFilter.blur(
                    sigmaX: kMaterialBlurMedium,
                    sigmaY: kMaterialBlurMedium,
                  ),
                  child: fill,
                ),
              )
            : fill,
      ),
    );

    if (!enabled) {
      circle = Opacity(opacity: 0.4, child: circle);
    }

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onPressed,
      onLongPress: onLongPress,
      child: SizedBox.square(
        dimension: hitSize,
        child: Center(child: circle),
      ),
    );
  }
}
