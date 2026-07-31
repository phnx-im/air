// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/constrained_width/constrained_width.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:provider/provider.dart';
import 'package:url_launcher/url_launcher.dart';

import 'package:air/features/user/user_cubit.dart';

class UpdateRequiredScreen extends StatelessWidget {
  const UpdateRequiredScreen({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final isOutdated = context.select(
      (UserCubit cubit) => cubit.state.unsupportedVersion,
    );
    final showUpdateButton = DeviceType.isPhone;
    return isOutdated
        ? UpdateRequiredView(showUpdateButton: showUpdateButton)
        : child;
  }
}

class UpdateRequiredView extends StatelessWidget {
  const UpdateRequiredView({super.key, required this.showUpdateButton});

  final bool showUpdateButton;

  @override
  Widget build(BuildContext context) {
    final colors = SemanticColors.of(context);
    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        automaticallyImplyLeading: false,
        title: Text(
          loc.appOutdatedScreen_title,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        backgroundColor: colors.backgroundBase.secondary,
      ),
      backgroundColor: colors.backgroundBase.secondary,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: S.s16),
          child: Center(
            child: ConstrainedWidth(
              child: Column(
                crossAxisAlignment: .center,
                children: [
                  const SizedBox(height: 3 * S.s96),

                  SizedBox(
                    width: 104,
                    child: SvgPicture.asset(
                      'assets/images/logo.svg',
                      colorFilter: ColorFilter.mode(
                        colors.text.primary,
                        BlendMode.srcIn,
                      ),
                    ),
                  ),

                  const SizedBox(height: 2 * S.s96),

                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: S.s16),
                    child: Text(
                      loc.appOutdatedScreen_message,
                      style: typeScale.header.l.style(
                        weight: Weight.emphasized,
                      ),
                      textAlign: .center,
                    ),
                  ),

                  const SizedBox(height: S.s16),

                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: S.s16),
                    child: Text(
                      loc.appOutdatedScreen_description,
                      style: typeScale.body.regular.style(
                        color: colors.text.secondary,
                      ),
                      textAlign: .center,
                    ),
                  ),

                  const Spacer(),

                  if (showUpdateButton)
                    Center(
                      child: Container(
                        padding: const EdgeInsets.symmetric(horizontal: S.s24),
                        width: context.breakpoint.isSmall
                            ? double.infinity
                            : null,
                        child: OutlinedButton(
                          onPressed: _handleUpdateNow,
                          style: OutlinedButtonTheme.of(context).style!
                              .copyWith(
                                backgroundColor: WidgetStateProperty.all(
                                  colors.accentBrand.primary,
                                ),
                                foregroundColor: WidgetStateProperty.all(
                                  colors.function.neutral.toggleWhite,
                                ),
                              ),
                          child: Text(
                            loc.appOutdatedScreen_action,
                            style: typeScale.body.regular.style(
                              color: colors.function.neutral.toggleWhite,
                            ),
                          ),
                        ),
                      ),
                    ),

                  const SizedBox(height: S.s16),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _handleUpdateNow() async {
    const String iOSAppStoreUrl =
        "https://beta.itunes.apple.com/v1/app/6749467927";
    const String androidPlayStoreUrl =
        "https://play.google.com/store/apps/details?id=ms.air";

    Uri url;

    if (Platform.isIOS) {
      url = Uri.parse(iOSAppStoreUrl);
    } else if (Platform.isAndroid) {
      url = Uri.parse(androidPlayStoreUrl);
    } else {
      return;
    }

    if (await canLaunchUrl(url)) {
      await launchUrl(url, mode: LaunchMode.externalApplication);
    }
  }
}
