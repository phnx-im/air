// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/desktop/width_constraints.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:provider/provider.dart';
import 'package:url_launcher/url_launcher.dart';

import 'user_cubit.dart';

class UpdateRequiredScreen extends StatelessWidget {
  const UpdateRequiredScreen({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final isOutdated = context.select(
      (UserCubit cubit) => cubit.state.unsupportedVersion,
    );
    final showUpdateButton = Platform.isIOS || Platform.isAndroid;
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
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        automaticallyImplyLeading: false,
        title: Text(
          loc.appOutdatedScreen_title,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        toolbarHeight: isPointer() ? 100 : null,
        backgroundColor: Colors.transparent,
      ),
      backgroundColor: colors.backgroundBase.secondary,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
          child: Center(
            child: ConstrainedWidth(
              child: Column(
                crossAxisAlignment: .center,
                children: [
                  const Spacer(),

                  Align(
                    alignment: Alignment.center,
                    child: SizedBox(
                      width: 104,
                      child: SvgPicture.asset(
                        'assets/images/logo.svg',
                        colorFilter: ColorFilter.mode(
                          colors.text.primary,
                          BlendMode.srcIn,
                        ),
                      ),
                    ),
                  ),

                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
                    child: Text(
                      loc.appOutdatedScreen_message,
                      style: TextStyle(
                        fontSize: HeaderFontSize.h2.size,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),

                  const SizedBox(height: Spacings.s),

                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
                    child: Text(
                      loc.appOutdatedScreen_description,
                      style: TextStyle(
                        fontSize: BodyFontSize.base.size,
                        color: colors.text.secondary,
                      ),
                    ),
                  ),

                  const Spacer(),

                  if (showUpdateButton)
                    Center(
                      child: Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: Spacings.m,
                        ),
                        width: isSmallScreen(context) ? double.infinity : null,
                        child: OutlinedButton(
                          onPressed: _handleUpdateNow,
                          style: OutlinedButtonTheme.of(context).style!
                              .copyWith(
                                backgroundColor: WidgetStateProperty.all(
                                  colors.accent.primary,
                                ),
                                foregroundColor: WidgetStateProperty.all(
                                  colors.function.toggleWhite,
                                ),
                              ),
                          child: Text(
                            loc.appOutdatedScreen_action,
                            style: TextStyle(
                              color: colors.function.toggleWhite,
                              fontSize: LabelFontSize.base.size,
                            ),
                          ),
                        ),
                      ),
                    ),

                  const SizedBox(height: Spacings.s),
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
