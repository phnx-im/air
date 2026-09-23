// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// The locale name to format dates and numbers with through `intl`.
///
/// `intl` ships symbols for `zh`, `zh_CN`, `zh_HK` and `zh_TW` and none for
/// the script-only `zh_Hant`, which it would resolve to the Simplified `zh`
/// data and write weekdays as 周一 rather than 週一. Every other supported
/// locale has symbols under its own name.
String intlLocaleName(Locale locale) {
  if (locale.languageCode == 'zh' && locale.scriptCode == 'Hant') {
    return 'zh_TW';
  }
  return locale.toString();
}
