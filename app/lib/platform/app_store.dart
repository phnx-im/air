// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:url_launcher/url_launcher.dart';

Future<void> openAppStore() async {
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
