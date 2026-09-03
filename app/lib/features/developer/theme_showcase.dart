// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/components/menu/menu_tokens.dart';
import 'package:air/ds/foundations/color_theme.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/developer/color_theme_cubit.dart';
import 'package:air/features/developer/color_theme_picker.dart';
import 'package:air/features/developer/theme_mode_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Floats a color theme picker over the whole app, on every screen, to flip
/// through the themes quickly. Prototype tooling: it sits above the router in
/// its own overlay, so it needs no navigator and survives every navigation.
class ThemeShowcase extends StatefulWidget {
  const ThemeShowcase({super.key, required this.child});

  final Widget child;

  @override
  State<ThemeShowcase> createState() => _ThemeShowcaseState();
}

class _ThemeShowcaseState extends State<ThemeShowcase> {
  late final OverlayEntry _content = OverlayEntry(builder: (_) => widget.child);
  // Above the navigator there is no Material, so the entries bring their own
  // for text and ink to resolve against.
  late final OverlayEntry _picker = OverlayEntry(
    builder: (_) => const Positioned(
      top: S.s8,
      right: S.s8,
      child: SafeArea(
        child: Material(
          type: MaterialType.transparency,
          child: Row(
            mainAxisSize: .min,
            spacing: S.s8,
            children: [_ModeToggle(), _FloatingPicker()],
          ),
        ),
      ),
    ),
  );

  @override
  void didUpdateWidget(ThemeShowcase oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.child != widget.child) _content.markNeedsBuild();
  }

  @override
  Widget build(BuildContext context) =>
      Overlay(initialEntries: [_content, _picker]);
}

/// Flips between light and dark, away from whatever is showing.
class _ModeToggle extends StatelessWidget {
  const _ModeToggle();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final brightness = Theme.of(context).brightness;
    return GestureDetector(
      behavior: .opaque,
      onTap: () => context.read<ThemeModeCubit>().toggle(brightness),
      child: Container(
        padding: const EdgeInsets.all(S.s4),
        decoration: BoxDecoration(
          color: palette.backgroundElevated.primary,
          border: Border.all(color: palette.separator.primary),
          borderRadius: BorderRadius.circular(CornerRadius.px8),
          boxShadow: Effect.elevation(Elevation.medium),
        ),
        child: Icon(
          brightness == Brightness.dark ? Icons.light_mode : Icons.dark_mode,
          size: S.s20,
          color: palette.text.primary,
        ),
      ),
    );
  }
}

/// The picker button. Its menu is an entry in the same overlay, dismissed by
/// a tap anywhere else.
class _FloatingPicker extends StatefulWidget {
  const _FloatingPicker();

  @override
  State<_FloatingPicker> createState() => _FloatingPickerState();
}

class _FloatingPickerState extends State<_FloatingPicker> {
  final GlobalKey _anchorKey = GlobalKey();
  OverlayEntry? _menu;

  @override
  void dispose() {
    _menu?.remove();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final selected = context.watch<ColorThemeCubit>().state;
    final palette = SemanticPalette.of(context);

    return GestureDetector(
      key: _anchorKey,
      behavior: .opaque,
      onTap: _toggle,
      child: Container(
        padding: const EdgeInsets.fromLTRB(S.s8, S.s4, S.s8, S.s4),
        decoration: BoxDecoration(
          color: palette.backgroundElevated.primary,
          border: Border.all(color: palette.separator.primary),
          borderRadius: BorderRadius.circular(CornerRadius.px8),
          boxShadow: Effect.elevation(Elevation.medium),
        ),
        child: Row(
          mainAxisSize: .min,
          spacing: S.s8,
          children: [
            ThemeSwatches(theme: selected),
            Text(
              selected.name,
              style: typeScale.body.regular.style(color: palette.text.primary),
            ),
            AppIcon.chevronDown(size: S.s16, color: palette.text.tertiary),
          ],
        ),
      ),
    );
  }

  void _toggle() {
    if (_menu != null) {
      _close();
      return;
    }
    final overlay = Overlay.of(context);
    final button = _anchorKey.currentContext?.findRenderObject();
    final theater = overlay.context.findRenderObject();
    if (button is! RenderBox || theater is! RenderBox) return;
    final anchor = Rect.fromPoints(
      theater.globalToLocal(button.localToGlobal(Offset.zero)),
      theater.globalToLocal(
        button.localToGlobal(button.size.bottomRight(Offset.zero)),
      ),
    );
    final cubit = context.read<ColorThemeCubit>();
    final selected = cubit.state;

    final entry = OverlayEntry(
      builder: (context) => Material(
        type: MaterialType.transparency,
        child: Stack(
          children: [
            Positioned.fill(
              child: GestureDetector(behavior: .opaque, onTap: _close),
            ),
            Positioned(
              top: anchor.bottom + S.s4,
              right: theater.size.width - anchor.right,
              child: Menu(
                tokens: MenuTokens.current,
                items: [
                  for (final theme in builtinColorThemes)
                    MenuItem(
                      label: theme.name,
                      leading: ThemeSwatches(theme: theme),
                      selected: theme.id == selected.id,
                      onPressed: () {
                        cubit.select(theme);
                        _close();
                      },
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
    _menu = entry;
    overlay.insert(entry);
  }

  void _close() {
    _menu?.remove();
    _menu = null;
  }
}
