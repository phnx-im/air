// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/nux/nux_pill.dart';
import 'package:air/ds/patterns/nux/nux_scaffold.dart';
import 'package:air/ds/patterns/nux/nux_scaffold_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:intl/intl.dart';
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
        : VersionExpiryBanner(child: child);
  }
}

/// Banner shown above the app content when the server announced that this
/// client's version expires soon. Dismissible until the next app start.
class VersionExpiryBanner extends StatefulWidget {
  const VersionExpiryBanner({super.key, required this.child});

  final Widget child;

  @override
  State<VersionExpiryBanner> createState() => _VersionExpiryBannerState();
}

class _VersionExpiryBannerState extends State<VersionExpiryBanner> {
  DateTime? _dismissedFor;

  @override
  Widget build(BuildContext context) {
    final expiresAt = context.select(
      (UserCubit cubit) => cubit.state.versionExpiresAt,
    );
    if (expiresAt == null || expiresAt == _dismissedFor) {
      return widget.child;
    }

    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);
    final locale = Localizations.localeOf(context).toString();
    final date = DateFormat.yMMMd(locale).format(expiresAt.toLocal());
    final onAccent = palette.function.neutral.toggleWhite;

    final banner = Material(
      color: palette.accentBrand.primary,
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
                  style: typeScale.body.s.style(color: onAccent),
                ),
              ),
              if (DeviceType.isPhone)
                TextButton(
                  onPressed: _handleUpdateNow,
                  child: Text(
                    loc.appOutdatedScreen_action,
                    style: typeScale.body.s.style(
                      color: onAccent,
                      weight: Weight.emphasized,
                    ),
                  ),
                ),
              IconButton(
                onPressed: () => setState(() => _dismissedFor = expiresAt),
                tooltip: loc.versionExpiryBanner_dismiss,
                icon: Icon(Icons.close, size: S.s16, color: onAccent),
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
          child: MediaQuery.removePadding(
            context: context,
            removeTop: true,
            child: widget.child,
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
          colorFilter: ColorFilter.mode(palette.text.primary, BlendMode.srcIn),
        ),
      ),
      footer: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
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
