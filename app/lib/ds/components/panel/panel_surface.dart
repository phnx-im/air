// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// The opaque color a desktop shell panel or pane effectively paints on.
///
/// A panel fill is translucent, so anything inside it that has to match the
/// surface it sits on -- a scroll fade gradient, text blended toward the
/// background -- can't use the fill directly: it has to composite over the
/// window beneath it. The shell publishes that composite here, and panes read
/// it instead of reaching for an opaque background tier that isn't what the
/// user sees. The content pane gets the window color the same way, as it runs
/// full-bleed on the window with no fill of its own.
class PanelSurface extends InheritedWidget {
  const PanelSurface({
    super.key,
    required this.color,
    this.ink,
    required super.child,
  });

  final Color color;

  /// Ink for content on an accent-filled surface, such as the selected chat
  /// row on `primary`. Null keeps the palette's text slots.
  final Color? ink;

  /// The ink of the surrounding surface, or null where the palette's text
  /// slots apply.
  static Color? inkOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<PanelSurface>()?.ink;

  /// The surrounding surface, or null outside the desktop shell (a full-screen
  /// phone layout).
  static Color? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<PanelSurface>()?.color;

  /// The surrounding surface, falling back to the base background tier where
  /// nothing publishes one.
  static Color colorOf(BuildContext context) =>
      maybeOf(context) ?? SemanticPalette.of(context).backgroundBase.primary;

  /// The text slots blended onto [colorOf], so each one is fully opaque. On
  /// an accent-filled surface the tiers are the surface's [ink] at the same
  /// emphasis steps.
  static TextPalette textOf(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final surface = context.dependOnInheritedWidgetOfExactType<PanelSurface>();
    final base = surface?.color ?? palette.backgroundBase.primary;
    final ink = surface?.ink;
    if (ink == null) return palette.text.on(base);
    return TextPalette(
      primary: ink,
      secondary: ink.withValues(alpha: 0.85),
      tertiary: ink.withValues(alpha: 0.7),
      quaternary: ink.withValues(alpha: 0.5),
    ).on(base);
  }

  @override
  bool updateShouldNotify(PanelSurface oldWidget) =>
      oldWidget.color != color || oldWidget.ink != ink;
}
