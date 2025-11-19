// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/theme/font.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/theme/theme.dart';
import 'package:air/util/notification_permissions.dart';
import 'package:flutter_svg/svg.dart';
import 'package:url_launcher/url_launcher.dart';

class IntroScreen extends StatelessWidget {
  const IntroScreen({super.key});

  static const double _logoWidth = 104;
  static final Uri _termsOfUseUri = Uri.parse('https://air.ms/terms');

  @override
  Widget build(BuildContext context) {
    final isUserLoading = context.select((LoadableUserCubit cubit) {
      return cubit.state is LoadingUser;
    });

    final loc = AppLocalizations.of(context);

    return Scaffold(
      backgroundColor: CustomColorScheme.of(context).backgroundBase.secondary,
      body: SafeArea(
        minimum: const EdgeInsets.symmetric(
          horizontal: Spacings.l,
          vertical: Spacings.l,
        ),
        child: Stack(
          children: [
            Align(
              alignment: Alignment.center,
              child: SizedBox(
                width: _logoWidth,
                child: GestureDetector(
                  onLongPress: () {
                    context.read<NavigationCubit>().openDeveloperSettings();
                  },
                  child: SvgPicture.asset(
                    'assets/images/logo.svg',
                    colorFilter: ColorFilter.mode(
                      CustomColorScheme.of(context).text.primary,
                      BlendMode.srcIn,
                    ),
                  ),
                ),
              ),
            ),
            Align(
              alignment: Alignment.bottomCenter,
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 420),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    _TermsOfUseText(loc: loc),
                    if (!isUserLoading) ...[
                      SizedBox(
                        width: double.infinity,
                        child: Padding(
                          padding: const EdgeInsets.only(top: Spacings.m),
                          child: OutlinedButton(
                            style: OutlinedButton.styleFrom(
                              textStyle: customTextScheme.labelMedium,
                              backgroundColor: CustomColorScheme.of(
                                context,
                              ).accent.primary,
                              foregroundColor: CustomColorScheme.of(
                                context,
                              ).function.toggleWhite,
                            ),
                            onPressed: () async {
                              await requestNotificationPermissionsIfNeeded();
                              if (!context.mounted) {
                                return;
                              }
                              context.read<NavigationCubit>().openSignUp();
                            },
                            child: Text(loc.introScreen_signUp),
                          ),
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _TermsOfUseText extends StatelessWidget {
  const _TermsOfUseText({required this.loc});

  final AppLocalizations loc;

  @override
  Widget build(BuildContext context) {
    final baseTextStyle = TextStyle(
      fontSize: LabelFontSize.small2.size,
      color: CustomColorScheme.of(context).text.tertiary,
    );

    final linkText = loc.introScreen_termsLinkText;
    final agreement = loc.introScreen_termsText(linkText);
    final linkStart = agreement.indexOf(linkText);

    if (linkStart == -1) {
      return Text(agreement, style: baseTextStyle, textAlign: TextAlign.center);
    }

    final beforeLink = agreement.substring(0, linkStart);
    final afterLink = agreement.substring(linkStart + linkText.length);

    final linkStyle = baseTextStyle.copyWith(
      color: CustomColorScheme.of(context).function.link,
    );

    return Text.rich(
      TextSpan(
        style: baseTextStyle,
        children: [
          TextSpan(text: beforeLink),
          TextSpan(
            text: linkText,
            style: linkStyle,
            recognizer: TapGestureRecognizer()
              ..onTap = () {
                launchUrl(
                  IntroScreen._termsOfUseUri,
                  mode: LaunchMode.externalApplication,
                );
              },
          ),
          TextSpan(text: afterLink),
        ],
      ),
      textAlign: TextAlign.center,
    );
  }
}
