// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/avatar/avatar_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// A round avatar: an [image] once it decodes, otherwise the first letter of
/// [displayName] on a coloured circle.
///
/// The fallback colour is a hash of [gradientSeed], so an account keeps the
/// same circle across every screen and two people sharing a name still differ.
class Avatar extends StatelessWidget {
  const Avatar({
    super.key,
    required this.displayName,
    required this.size,
    this.image,
    this.gradientSeed,
    this.onTap,
  });

  /// Source of the fallback initial. Pass an empty string for a circle that
  /// carries no letter.
  final String displayName;

  /// Diameter of the circle.
  final double size;

  /// Picture covering the circle. Decoding it is the host's business, so it
  /// arrives resolved rather than as a URL or a blob.
  final ImageProvider? image;

  /// Seeds the fallback hue. Null lands on the first one.
  final String? gradientSeed;

  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    const tokens = AvatarTokens.standard;
    final palette = SemanticPalette.of(context);
    final image = this.image;

    // A picture covers the circle, so the fill under it only shows while it
    // decodes: a flat tint reads as a placeholder, where the gradient would
    // read as a fallback that then swaps out.
    final circle = Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        shape: .circle,
        gradient: image == null ? tokens.gradientFor(gradientSeed) : null,
        color: image == null ? null : palette.text.quaternary,
      ),
      foregroundDecoration: image == null
          ? null
          : BoxDecoration(
              shape: .circle,
              image: DecorationImage(image: image, fit: .cover),
            ),
      child: Center(
        // We size the letter off the circle, so text scaling would spill it
        // over the edge instead of enlarging it.
        child: MediaQuery.withNoTextScaling(
          child: Text(
            displayName.characters.firstOrNull?.toUpperCase() ?? "",
            style: typeScale.body.regular
                .style(color: palette.function.neutral.white)
                .copyWith(fontSize: tokens.letterSize(size)),
          ),
        ),
      ),
    );

    if (onTap == null) {
      return circle;
    }

    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(onTap: onTap, child: circle),
    );
  }
}
