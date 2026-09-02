// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/l10n/l10n.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/nux/nux_pill.dart';
import 'package:air/ds/patterns/nux/nux_scaffold.dart';
import 'package:air/ds/patterns/nux/nux_scaffold_tokens.dart';
import 'package:air/util/time/time_labels.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:provider/provider.dart';
import 'package:url_launcher/url_launcher.dart';

import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';

class UpdateRequiredScreen extends StatelessWidget {
  const UpdateRequiredScreen({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final versionStatus = context.select(
      (UserCubit cubit) => cubit.state.versionStatus,
    );
    return switch (versionStatus) {
      VersionStatus_Supported() => child,
      VersionStatus_ExpiresAt(field0: final expiresAt) => VersionExpiryBanner(
        expiresAt: expiresAt,
        child: child,
      ),
      VersionStatus_Unsupported() => UpdateRequiredView(
        showUpdateButton: DeviceType.isPhone,
      ),
    };
  }
}

/// Banner above the app content when the server announced that this
/// version stops being accepted at [expiresAt]. Dismissing it is persisted
/// per announced expiry, so a later announcement shows it again.
class VersionExpiryBanner extends StatelessWidget {
  const VersionExpiryBanner({
    super.key,
    required this.expiresAt,
    required this.child,
  });

  final DateTime expiresAt;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final dismissedFor = context.select(
      (UserSettingsCubit cubit) => cubit.state.dismissedVersionExpiry,
    );
    if (dismissedFor != null && dismissedFor.isAtSameMomentAs(expiresAt)) {
      return child;
    }

    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);
    final date = TimeFormats.of(context).monthDayYear(expiresAt.toLocal());

    final banner = Material(
      color: palette.function.warning.primary,
      child: SafeArea(
        bottom: false,
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: S.s16,
            vertical: S.s4,
          ),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  loc.versionExpiryBanner_message(date),
                  style: typeScale.body.s.style(),
                ),
              ),
              // Only a store has an update to send us to, so a desktop build
              // shows no button.
              if (DeviceType.isPhone) ...[
                const SizedBox(width: S.s8),
                Button(
                  size: ButtonSize.small,
                  onPressed: _handleUpdateNow,
                  label: loc.appOutdatedScreen_action,
                ),
              ],
              Tooltip(
                message: loc.versionExpiryBanner_dismiss,
                child: ButtonIcon(
                  variant: ButtonIconVariant.plain,
                  icon: AppIconType.x,
                  onPressed: () => context
                      .read<UserSettingsCubit>()
                      .setDismissedVersionExpiry(value: expiresAt),
                ),
              ),
            ],
          ),
        ),
      ),
    );

    return Column(
      children: [
        banner,
        Expanded(
          // The banner already took the status bar inset.
          child: MediaQuery.removePadding(
            context: context,
            removeTop: true,
            child: child,
          ),
        ),
      ],
    );
  }
}

class UpdateRequiredView extends StatelessWidget {
  const UpdateRequiredView({super.key, required this.showUpdateButton});

  final bool showUpdateButton;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);

    return NuxScaffold(
      tokens: NuxScaffoldTokens.of(context),
      // The same slot as the start screen's language picker, so the two
      // signed-out screens read as one place.
      top: NuxPill(
        icon: AppIconType.circleAlert,
        label: loc.appOutdatedScreen_title,
      ),
      body: SizedBox(
        width: 104,
        child: SvgPicture.asset(
          'assets/images/logo.svg',
          colorFilter: ColorFilter.mode(palette.text.primary, .srcIn),
        ),
      ),
      footer: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: [
          Text(
            loc.appOutdatedScreen_message,
            style: typeScale.header.l.style(weight: Weight.emphasized),
            textAlign: .center,
          ),

          const SizedBox(height: S.s16),

          Text(
            loc.appOutdatedScreen_description,
            style: typeScale.body.regular.style(color: palette.text.secondary),
            textAlign: .center,
          ),

          // Only a store has an update to send us to, so a desktop build
          // shows no button.
          if (showUpdateButton) ...[
            const SizedBox(height: S.s24),
            Button(
              onPressed: _handleUpdateNow,
              label: loc.appOutdatedScreen_action,
            ),
          ],
        ],
      ),
    );
  }
}

void _handleUpdateNow() async {
  // Country-neutral, so the store resolves the user's own storefront.
  const String iOSAppStoreUrl = "https://apps.apple.com/app/id6749467927";
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
