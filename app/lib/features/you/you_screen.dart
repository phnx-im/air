// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' show max;

import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/ds/components/scroll/faded_scroll_frame.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/navigation_state.dart';
import 'package:air/features/navigation/tab_bar_tokens.dart';
import 'package:air/features/user/avatar.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/features/you/invitation_codes_cubit.dart';
import 'package:air/features/you/you_fade_tokens.dart';
import 'package:air/features/you/you_menu.dart';
import 'package:air/features/you/you_sections.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:image_picker/image_picker.dart';

/// The profile tab on a phone: who you are, above the sections you can open.
///
/// Each row drills into its own screen ([YouSectionScreen]); the two-pane
/// layout shows the same sections side by side instead, see `YouMenuPane`.
class YouScreen extends StatelessWidget {
  const YouScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final bgColor = palette.backgroundBase.primary;

    return Scaffold(
      backgroundColor: bgColor,
      body: SafeArea(
        bottom: false,
        child: FadedScrollFrame(
          backgroundColor: bgColor,
          header: _Header(title: loc.userSettingsScreen_title),
          contentTopPadding: Chrome.barHeight,
          // The floating tab bar is what the last row has to clear, unless the
          // bottom fade reaches further down.
          contentBottomPadding: max(
            TabBarTokens.bottomInset(context),
            YouFadeTokens.phone.bottomHeight,
          ),
          topFadeHeight: YouFadeTokens.phone.topHeight,
          bottomFadeHeight: YouFadeTokens.phone.bottomHeight,
          topSolidStop: YouFadeTokens.topSolidStop,
          bottomSolidStop: YouFadeTokens.bottomSolidStop,
          topOpacity: YouFadeTokens.topOpacity,
          bottomOpacity: YouFadeTokens.bottomOpacity,
          builder: (topPadding, bottomPadding) => SingleChildScrollView(
            padding: EdgeInsets.only(
              top: topPadding,
              bottom: bottomPadding,
              left: S.s16,
              right: S.s16,
            ),
            child: const Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(height: S.s16),
                _Identity(),
                SizedBox(height: S.s24),
                YouMenu.sectionList(),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// One section, pushed over the phone's section list.
class YouSectionScreen extends StatelessWidget {
  const YouSectionScreen({super.key, required this.section});

  final YouSection section;

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) =>
          InvitationCodesCubit(userCubit: context.read<UserCubit>()),
      child: YouSectionView(section: section),
    );
  }
}

class YouSectionView extends StatelessWidget {
  const YouSectionView({super.key, required this.section});

  final YouSection section;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return AppScaffold(
      title: youSectionTitle(loc, section),
      backgroundColor: SemanticPalette.of(context).backgroundBase.primary,
      child: YouSectionContent(section: section),
    );
  }
}

/// Profile picture and display name, above the section list.
class _Identity extends StatelessWidget {
  const _Identity();

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: null),
    );
    final palette = SemanticPalette.of(context);

    return Column(
      children: [
        UserAvatar(
          profile: profile,
          size: S.s160,
          onPressed: () => _pickAvatar(context),
        ),
        const SizedBox(height: S.s12),
        Text(
          profile.displayName,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: typeScale.body.m.style(
            weight: Weight.emphasized,
            color: palette.text.primary,
          ),
        ),
      ],
    );
  }

  void _pickAvatar(BuildContext context) async {
    final user = context.read<UserCubit>();

    final ImagePicker picker = ImagePicker();
    // Reduce image quality to re-encode the image.
    final XFile? image = await picker.pickImage(
      source: ImageSource.gallery,
      imageQuality: 99,
    );
    final bytes = await image?.readAsBytes();

    if (bytes != null) {
      await user.setProfile(profilePicture: bytes);
    }
  }
}

class _Header extends StatelessWidget {
  const _Header({required this.title});

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
