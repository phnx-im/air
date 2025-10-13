// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:package_info_plus/package_info_plus.dart';

import 'contact_us_screen.dart';
import 'user_settings_screen.dart';

class HelpScreen extends StatelessWidget {
  const HelpScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(loc.helpScreen_title),
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        child: Align(
          alignment: Alignment.topCenter,
          child: Container(
            constraints:
                isPointer() ? const BoxConstraints(maxWidth: 800) : null,
            child: ListView(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              children: const [
                _ContactUs(),

                SettingsDivider(),

                _VersionInfo(),
                _Licenses(),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ContactUs extends StatelessWidget {
  const _ContactUs();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return ListTile(
      title: Text(loc.helpScreen_contactUs),
      onTap:
          () => Navigator.of(context).push(
            MaterialPageRoute(builder: (context) => const ContactUsScreen()),
          ),
    );
  }
}

class _VersionInfo extends HookWidget {
  const _VersionInfo();

  @override
  Widget build(BuildContext context) {
    final packageInfoFut = useMemoized(() => PackageInfo.fromPlatform());
    final packageInfoSnap = useFuture(packageInfoFut);

    final packageInfo = packageInfoSnap.data;
    if (packageInfo == null) {
      return const SizedBox();
    }

    final version = "${packageInfo.version}-${packageInfo.buildNumber}";

    final loc = AppLocalizations.of(context);

    return ListTile(
      title: Text(loc.helpScreen_versionInfo),
      subtitle: Text(
        version,
        style: TextStyle(color: CustomColorScheme.of(context).text.tertiary),
      ),
    );
  }
}

class _Licenses extends StatelessWidget {
  const _Licenses();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ListTile(
      title: Text(loc.helpScreen_licenses),
      onTap: () {
        Navigator.of(
          context,
        ).push(MaterialPageRoute(builder: (context) => const LicensePage()));
      },
    );
  }
}
