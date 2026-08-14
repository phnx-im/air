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
    final palette = SemanticPalette.of(context);
    final image = this.image;

    // One decoration, not a background plus a foreground. A BoxDecoration
    // paints its color and then its image, which is the same result as laying
    // the picture over a tinted circle, for one render object instead of two.
    // The tint only shows while the picture decodes: a flat one reads as a
    // placeholder, where the gradient would read as a fallback that then swaps
    // out.
    final circle = DecoratedBox(
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: image == null ? AvatarTokens.gradientFor(gradientSeed) : null,
        color: image == null ? null : palette.text.quaternary,
        image: image == null
            ? null
            : DecorationImage(image: image, fit: BoxFit.cover),
      ),
      child: SizedBox.square(
        dimension: size,
        // Only the letter, and only where it can be seen. Under a picture it is
        // a grapheme break, a paragraph layout and a MediaQuery dependency for
        // something never painted.
        child: image != null ? null : _Letter(displayName: displayName, size: size),
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

/// The fallback initial, drawn only when no picture covers it.
class _Letter extends StatelessWidget {
  const _Letter({required this.displayName, required this.size});

  final String displayName;
  final double size;

  @override
  Widget build(BuildContext context) {
    // Grapheme clusters rather than code units, so an emoji or a combining
    // mark yields one character instead of half of one.
    final initial = displayName.characters.firstOrNull?.toUpperCase();
    if (initial == null || initial.isEmpty) {
      return const SizedBox.shrink();
    }

    return Center(
      child: Text(
        initial,
        // We size the letter off the circle, so text scaling would spill it
        // over the edge instead of enlarging it. Set on the Text rather than
        // through MediaQuery.withNoTextScaling, which costs a Builder, a
        // dependency on the whole MediaQueryData and a copy of it.
        textScaler: TextScaler.noScaling,
        style: typeScale.body.regular
            .style(color: SemanticPalette.of(context).function.neutral.white)
            .copyWith(fontSize: AvatarTokens.letterSize(size)),
      ),
    );
  }
}
