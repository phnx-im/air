// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/l10n/app_locale_cubit.dart';
import 'package:air/l10n/language_options.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/context_menu/context_menu.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

typedef LanguagePickerChildBuilder =
    Widget Function(
      BuildContext context,
      LanguageOption option,
      VoidCallback onTap,
    );

class LanguagePickerMenu extends StatefulWidget {
  const LanguagePickerMenu({
    super.key,
    this.direction = ContextMenuDirection.right,
    required this.onLocaleSelected,
    required this.childBuilder,
  });

  final ContextMenuDirection direction;
  final Future<void> Function(Locale locale) onLocaleSelected;
  final LanguagePickerChildBuilder childBuilder;

  @override
  State<LanguagePickerMenu> createState() => _LanguagePickerMenuState();
}

class _LanguagePickerMenuState extends State<LanguagePickerMenu> {
  final OverlayPortalController _contextMenuController =
      OverlayPortalController();

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final storedLocale = context.select(
      (UserSettingsCubit cubit) => cubit.state.locale,
    );
    final overrideLocale = context.select(
      (AppLocaleCubit cubit) => cubit.state,
    );
    // Use persisted user preference when available, otherwise fall back to
    // in-memory selection or device locale.
    final languageOptions = buildLanguageOptions();
    final resolvedLocale = supportedLanguageLocale(
      localeForLanguageCode(storedLocale) ??
          overrideLocale ??
          Localizations.localeOf(context),
    );
    final currentOption = languageOptions.firstWhere(
      (option) => option.locale.languageCode == resolvedLocale.languageCode,
      orElse: () => languageOptions.first,
    );

    final menuItems = <ContextMenuEntry>[];
    for (final option in languageOptions) {
      menuItems.add(
        ContextMenuItem(
          label: option.label,
          leading: option.locale.languageCode == resolvedLocale.languageCode
              ? AppIcon.check(size: 16, color: colors.text.secondary)
              : null,
          onPressed: () {
            unawaited(widget.onLocaleSelected(option.locale));
          },
        ),
      );
    }

    return ContextMenu(
      direction: widget.direction,
      controller: _contextMenuController,
      menuItems: menuItems,
      child: widget.childBuilder(
        context,
        currentOption,
        () => _contextMenuController.show(),
      ),
    );
  }
}
