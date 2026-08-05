// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/you/you_sections.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Height of one menu row.
const _rowHeight = S.s40;

/// The profile sections as icon and label rows.
///
/// The default is the two-pane form: a flat list filling the panel, where the
/// selected row carries a persistent fill because the section it names is open
/// beside it. [YouMenu.sectionList] is the phone form, where the same rows are
/// grouped into cards with a trailing chevron and each one drills into its
/// section.
class YouMenu extends StatelessWidget {
  const YouMenu({super.key}) : _sectionList = false;

  const YouMenu.sectionList({super.key}) : _sectionList = true;

  final bool _sectionList;

  @override
  Widget build(BuildContext context) {
    final isDeveloper = context.select(
      (UserSettingsCubit cubit) => cubit.state.isDeveloper,
    );
    final activeSection = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        NavigationState_Home(:final home) => home.youSection,
        NavigationState_Intro() => null,
      },
    );

    // Linking a device is still developer-only, and so is everything the
    // developer settings expose.
    final groups = [
      [
        YouSection.profile,
        if (isDeveloper) YouSection.devices,
        YouSection.account,
      ],
      [YouSection.preferences, YouSection.help],
    ];

    if (_sectionList) {
      return _SectionList(
        groups: groups,
        activeSection: activeSection,
        showDeveloper: isDeveloper,
      );
    }
    return _FlatMenu(
      sections: groups.expand((group) => group).toList(),
      activeSection: activeSection,
      showDeveloper: isDeveloper,
    );
  }
}

/// The two-pane form: every row in one inline list, no cards, no chevrons.
class _FlatMenu extends StatelessWidget {
  const _FlatMenu({
    required this.sections,
    required this.activeSection,
    required this.showDeveloper,
  });

  final List<YouSection> sections;
  final YouSection? activeSection;
  final bool showDeveloper;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    // With no list of its own to fall back to, the pane always shows a
    // section, so the menu marks the profile as selected until another one is
    // opened.
    final selected = activeSection ?? YouSection.profile;

    return Padding(
      padding: const EdgeInsets.all(S.s8),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          for (final section in sections)
            _MenuRow(
              icon: _sectionIcon(section),
              label: youSectionTitle(loc, section),
              selected: section == selected,
              onTap: () =>
                  context.read<NavigationCubit>().openYouSection(section),
            ),
          if (showDeveloper)
            _MenuRow(
              icon: AppIconType.squareTerminal,
              label: loc.youSection_developer,
              onTap: () =>
                  context.read<NavigationCubit>().openDeveloperSettings(),
            ),
        ],
      ),
    );
  }
}

/// The phone form: grouped cards of filled rows, each drilling into a section.
class _SectionList extends StatelessWidget {
  const _SectionList({
    required this.groups,
    required this.activeSection,
    required this.showDeveloper,
  });

  final List<List<YouSection>> groups;
  final YouSection? activeSection;
  final bool showDeveloper;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    Widget card(List<Widget> rows) => ClipRRect(
      borderRadius: BorderRadius.circular(CornerRadius.px16),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          for (final (index, row) in rows.indexed) ...[
            // A hairline of the screen behind separates the rows, so the card
            // reads as one shape without a drawn divider in it.
            if (index > 0) const SizedBox(height: StrokeWidth.px1),
            row,
          ],
        ],
      ),
    );

    return Column(
      crossAxisAlignment: .stretch,
      children: [
        for (final (index, group) in groups.indexed) ...[
          if (index > 0) const SizedBox(height: S.s16),
          card([
            for (final section in group)
              _MenuRow(
                icon: _sectionIcon(section),
                label: youSectionTitle(loc, section),
                filled: true,
                chevron: true,
                onTap: () =>
                    context.read<NavigationCubit>().openYouSection(section),
              ),
          ]),
        ],
        if (showDeveloper) ...[
          const SizedBox(height: S.s16),
          card([
            _MenuRow(
              icon: AppIconType.squareTerminal,
              label: loc.youSection_developer,
              filled: true,
              chevron: true,
              onTap: () =>
                  context.read<NavigationCubit>().openDeveloperSettings(),
            ),
          ]),
        ],
      ],
    );
  }
}

class _MenuRow extends StatelessWidget {
  const _MenuRow({
    required this.icon,
    required this.label,
    required this.onTap,
    this.selected = false,
    this.filled = false,
    this.chevron = false,
  });

  final AppIconType icon;
  final String label;
  final VoidCallback onTap;

  /// Whether this row names the section currently shown beside it.
  final bool selected;

  /// Whether the row paints its own fill, as the phone cards do. The two-pane
  /// list leaves the panel showing through instead.
  final bool filled;

  final bool chevron;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final radius = filled ? CornerRadius.px0 : CornerRadius.px12;
    final fill = switch ((filled, selected)) {
      (true, _) => palette.backgroundBase.secondary,
      (false, true) => palette.backgroundBase.quinary,
      (false, false) => null,
    };

    return StateLayer(
      onTap: onTap,
      borderRadius: radius,
      // The wash has to sit on what the user sees: an unfilled row shows the
      // panel behind it, which is translucent.
      surface:
          fill ??
          PanelSurface.maybeOf(context) ??
          palette.backgroundBase.primary,
      hover: !DeviceType.isPhone,
      pressScale: false,
      background: fill != null
          ? DecoratedBox(
              decoration: BoxDecoration(
                color: fill,
                borderRadius: BorderRadius.circular(radius),
              ),
            )
          : null,
      child: SizedBox(
        height: _rowHeight,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: S.s12),
          child: Row(
            children: [
              AppIcon(type: icon, size: S.s20, color: palette.text.secondary),
              const SizedBox(width: S.s12),
              Expanded(
                child: Text(
                  label,
                  maxLines: 1,
                  overflow: .ellipsis,
                  style: typeScale.body.regular.style(
                    color: palette.text.primary,
                  ),
                ),
              ),
              if (chevron)
                AppIcon.chevronRight(size: S.s16, color: palette.text.tertiary),
            ],
          ),
        ),
      ),
    );
  }
}

AppIconType _sectionIcon(YouSection section) => switch (section) {
  YouSection.profile => AppIconType.user,
  YouSection.devices => AppIconType.laptop,
  YouSection.account => AppIconType.key,
  YouSection.preferences => AppIconType.settings,
  YouSection.help => AppIconType.circleHelp,
};
