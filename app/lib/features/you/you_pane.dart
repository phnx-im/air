// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/components/scroll/faded_scroll_frame.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/you/invitation_codes_cubit.dart';
import 'package:air/features/you/you_fade_tokens.dart';
import 'package:air/features/you/you_menu.dart';
import 'package:air/features/you/you_sections.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// The section list of the profile tab, filling the list panel of the two-pane
/// layout. The section it selects opens in [YouDetailPane] beside it.
class YouMenuPane extends StatelessWidget {
  const YouMenuPane({super.key});

  @override
  Widget build(BuildContext context) {
    final surface =
        PanelSurface.maybeOf(context) ??
        SemanticPalette.of(context).backgroundBase.secondary;

    return ColoredBox(
      color: surface,
      child: const SingleChildScrollView(child: YouMenu()),
    );
  }
}

/// The open section of the profile tab, filling the content pane of the
/// two-pane layout.
class YouDetailPane extends StatelessWidget {
  const YouDetailPane({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    // The pane has no list of its own to fall back to, so it always shows a
    // section.
    final section =
        context.select(
          (NavigationCubit cubit) => switch (cubit.state) {
            HomeState(:final home) => home.youSection,
            IntroState() => null,
          },
        ) ??
        YouSection.profile;

    // The pane sits on the window rather than on a panel, so its fades land on
    // the window color.
    final background = palette.backgroundBase.quinary;

    return BlocProvider(
      create: (context) =>
          InvitationCodesCubit(userCubit: context.read<UserCubit>()),
      child: FadedScrollFrame(
        backgroundColor: background,
        header: _PaneHeader(title: youSectionTitle(loc, section)),
        topFadeHeight: YouFadeTokens.desktop.topHeight,
        bottomFadeHeight: YouFadeTokens.desktop.bottomHeight,
        topSolidStop: YouFadeTokens.topSolidStop,
        bottomSolidStop: YouFadeTokens.bottomSolidStop,
        bottomOpacity: YouFadeTokens.bottomOpacity,
        contentTopPadding: Chrome.barHeight,
        contentBottomPadding: Chrome.barHeight,
        builder: (topPadding, bottomPadding) => SingleChildScrollView(
          padding: EdgeInsets.only(top: topPadding, bottom: bottomPadding),
          child: Center(
            child: ConstrainedBox(
              // Widest the section content grows to before it stops following
              // the pane.
              constraints: const BoxConstraints(maxWidth: Measure.m800),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: S.s16),
                child: YouSectionContent(section: section),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _PaneHeader extends StatelessWidget {
  const _PaneHeader({required this.title});

  final String title;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: Chrome.barHeight,
      child: Center(
        child: Text(
          title,
          style: typeScale.body.regular.style(weight: Weight.emphasized),
        ),
      ),
    );
  }
}
