// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/theme/theme.dart';
import 'package:flutter_svg/svg.dart';

class IntroScreen extends StatelessWidget {
  const IntroScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final isUserLoading = context.select((LoadableUserCubit cubit) {
      return cubit.state is LoadingUser;
    });

    final loc = AppLocalizations.of(context);

    return Scaffold(
      body: SafeArea(
        minimum: const EdgeInsets.only(
          left: Spacings.m,
          right: Spacings.m,
          bottom: Spacings.l + Spacings.xxs,
        ),
        child: Center(
          child: Column(
            children: [
              const Spacer(),
              GestureDetector(
                onLongPress: () {
                  context.read<NavigationCubit>().openDeveloperSettings();
                },
                child: SizedBox(
                  width: 64,
                  height: 64,
                  child: SvgPicture.asset(
                    'assets/images/tilde.svg',
                    width: 64,
                    colorFilter: ColorFilter.mode(
                      CustomColorScheme.of(context).text.primary,
                      BlendMode.srcIn,
                    ),
                  ),
                ),
              ),
              const Spacer(),
              if (!isUserLoading)
                Column(
                  crossAxisAlignment:
                      isSmallScreen(context)
                          ? CrossAxisAlignment.stretch
                          : CrossAxisAlignment.center,
                  children: [
                    OutlinedButton(
                      onPressed:
                          () =>
                              context
                                  .read<NavigationCubit>()
                                  .openServerChoice(),
                      child: Text(loc.introScreen_signUp),
                    ),
                  ],
                ),
            ],
          ),
        ),
      ),
    );
  }
}
