// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/foundations/color_theme.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/features/developer/color_theme_cubit.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// A dropdown over the built-in color themes: the trigger shows the selected
/// theme's swatches and name, the menu lists every theme the same way.
class ColorThemePicker extends StatefulWidget {
  const ColorThemePicker({super.key});

  @override
  State<ColorThemePicker> createState() => _ColorThemePickerState();
}

class _ColorThemePickerState extends State<ColorThemePicker> {
  final GlobalKey _anchorKey = GlobalKey();

  @override
  Widget build(BuildContext context) {
    final selected = context.watch<ColorThemeCubit>().state;
    final palette = SemanticPalette.of(context);

    return GestureDetector(
      key: _anchorKey,
      behavior: .opaque,
      onTap: () => _open(context, selected),
      child: Container(
        padding: const EdgeInsets.fromLTRB(S.s8, S.s4, S.s8, S.s4),
        decoration: BoxDecoration(
          border: Border.all(color: palette.separator.primary),
          borderRadius: BorderRadius.circular(CornerRadius.px8),
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

  void _open(BuildContext context, ColorTheme selected) {
    final render = _anchorKey.currentContext?.findRenderObject();
    if (render is! RenderBox || !render.hasSize) return;
    final cubit = context.read<ColorThemeCubit>();

    unawaited(
      showOverlayMenu(
        context: context,
        anchor: render.localToGlobal(Offset.zero) & render.size,
        corner: MenuCorner.topRight,
        items: [
          for (final theme in builtinColorThemes)
            MenuItem(
              label: theme.name,
              leading: ThemeSwatches(theme: theme),
              selected: theme.id == selected.id,
              onPressed: () => cubit.select(theme),
            ),
        ],
      ),
    );
  }
}

/// Four accent dots on the theme's own dark background.
/// The theme's four preview dots on its dark background.
class ThemeSwatches extends StatelessWidget {
  const ThemeSwatches({super.key, required this.theme});

  final ColorTheme theme;

  static const double _dot = 10.0;

  @override
  Widget build(BuildContext context) {
    final primitives = theme.primitives;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: S.s4, vertical: S.s2),
      decoration: BoxDecoration(
        color: theme.darkBackground,
        borderRadius: BorderRadius.circular(_dot),
      ),
      child: Row(
        mainAxisSize: .min,
        spacing: S.s2,
        children: [
          for (final hue in theme.swatches)
            DecoratedBox(
              decoration: BoxDecoration(
                shape: .circle,
                color: primitives.chromatic(hue, Shade.s400),
              ),
              child: const SizedBox.square(dimension: _dot),
            ),
        ],
      ),
    );
  }
}
