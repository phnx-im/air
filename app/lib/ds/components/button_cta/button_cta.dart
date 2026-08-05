// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_cta/button_cta_tokens.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Round action button carrying its label underneath, for the row of actions a
/// details surface leads with.
///
/// The circle is a [ButtonIcon] with a per-type fill, so hover / press / focus
/// stay identical to every other round button in the system.
///
/// Without a handler the button reads as disabled: [ButtonIcon] fades the
/// circle, and we fade the label by the same alpha, so the two read as one
/// inert unit rather than a live label under a ghosted circle.
class ButtonCTA extends StatelessWidget {
  const ButtonCTA({
    super.key,
    required this.tokens,
    required this.label,
    required this.icon,
    this.onPressed,
    this.type = ButtonCTAType.primary,
  });

  final ButtonCTATokens tokens;
  final String label;
  final AppIconType icon;
  final VoidCallback? onPressed;
  final ButtonCTAType type;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final (fill, glyph) = switch (type) {
      ButtonCTAType.primary => (
        palette.accentBrand.primary,
        palette.backgroundBase.primary,
      ),
      ButtonCTAType.secondary => (
        palette.fill.tertiary,
        palette.text.secondary,
      ),
    };

    final labelColor = palette.text.tertiary;
    final fade = onPressed != null ? 1.0 : ButtonIconTokens.disabledOpacity;

    return Column(
      mainAxisSize: .min,
      children: [
        ButtonIcon(
          variant: ButtonIconVariant.solid,
          icon: icon,
          size: tokens.size,
          iconSize: tokens.iconSize,
          iconColor: glyph,
          fill: fill,
          onPressed: onPressed,
        ),
        SizedBox(height: tokens.labelGap),
        Text(
          label,
          style: typeScale.body.regular
              .style(color: labelColor.withValues(alpha: labelColor.a * fade))
              .copyWith(height: 1.0),
        ),
      ],
    );
  }
}
