// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/hover_action/hover_action_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// The bubble a hover action belongs to. The button carries that bubble's
/// fill, so it reads as part of the message rather than as chrome floating
/// beside it.
enum HoverActionSurface { self, other }

/// A reveal-on-hover round icon button: a single glyph on the fill of the
/// message it belongs to, triggering a quick action such as react or reply.
///
/// Just the button. The slot beside the bubble and *when* to reveal it are
/// layout the host owns, so it toggles [revealed] and the button fades itself
/// in and out. Hidden it's transparent and inert, but it still occupies its
/// slot, so revealing it never shifts what sits around it.
class HoverAction extends StatelessWidget {
  const HoverAction({
    super.key,
    required this.tokens,
    required this.icon,
    required this.surface,
    this.revealed = true,
    this.onPressed,
  });

  final HoverActionTokens tokens;

  /// The action the button stands for.
  final AppIconType icon;

  final HoverActionSurface surface;

  /// Whether the button is shown. Defaults to true for a host that places the
  /// button unconditionally.
  final bool revealed;

  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final fill = switch (surface) {
      HoverActionSurface.self => palette.message.selfBackground,
      HoverActionSurface.other => palette.message.otherBackground,
    };

    return AnimatedScale(
      scale: revealed ? 1.0 : tokens.hiddenScale,
      duration: Effect.duration(tokens.revealMotion),
      curve: Effect.easeOutQuart,
      child: AnimatedOpacity(
        opacity: revealed ? Alpha.a100 : Alpha.a0,
        duration: Effect.duration(tokens.revealMotion),
        curve: Effect.easeOutQuart,
        // A transparent button still hit-tests, so it would swallow a click
        // meant for the bubble underneath it.
        child: IgnorePointer(
          ignoring: !revealed,
          child: ButtonIcon(
            variant: ButtonIconVariant.solid,
            size: tokens.size,
            fill: fill,
            icon: icon,
            iconSize: tokens.glyphSize,
            iconColor: palette.text.secondary,
            onPressed: onPressed,
          ),
        ),
      ),
    );
  }
}
