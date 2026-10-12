// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/l10n/app_locale_cubit.dart';
import 'package:air/l10n/language_options.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

typedef LanguagePickerChildBuilder = Widget Function(
  BuildContext context,
  LanguageOption option,
  VoidCallback onTap,
);

class LanguagePickerMenu extends StatelessWidget {
  const LanguagePickerMenu({
    super.key,
    required this.onLocaleSelected,
    required this.childBuilder,
  });

  final Future<void> Function(Locale locale) onLocaleSelected;
  final LanguagePickerChildBuilder childBuilder;

  @override
  Widget build(BuildContext context) {
    final storedLocale = context.select(
      (UserSettingsCubit cubit) => cubit.state.locale,
    );
    final overrideLocale = context.select(
      (AppLocaleCubit cubit) => cubit.state,
    );
    // Use persisted user preference when available, otherwise fall back to
    // in-memory selection or device locale.
    final languageOptions = buildLanguageOptions();
    final resolvedLocale = resolveSupportedLocale(
      localeFromTag(storedLocale) ??
          overrideLocale ??
          Localizations.localeOf(context),
    );
    final currentOption = languageOptions.firstWhere(
      (option) => option.locale == resolvedLocale,
      orElse: () => languageOptions.first,
    );

    return Builder(
      builder: (anchorContext) => childBuilder(
        anchorContext,
        currentOption,
        () => _open(anchorContext, languageOptions, resolvedLocale),
      ),
    );
  }

  void _open(
    BuildContext anchorContext,
    List<LanguageOption> options,
    Locale resolvedLocale,
  ) {
    final render = anchorContext.findRenderObject();
    if (render is! RenderBox || !render.hasSize) return;

    unawaited(
      showOverlayMenu(
        context: anchorContext,
        anchor: render.localToGlobal(Offset.zero) & render.size,
        items: [
          for (final option in options)
            MenuItem(
              label: option.label,
              selected: option.locale == resolvedLocale,
              onPressed: () => unawaited(onLocaleSelected(option.locale)),
            ),
        ],
      ),
    );
  }
}
