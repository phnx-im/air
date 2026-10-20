// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/app_localizations.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Ukrainian picks the one, few and many forms by the last digits', () {
    final loc = lookupAppLocalizations(const Locale('uk'));
    expect(loc.groupDetails_memberCount(1), '1 людина');
    expect(loc.groupDetails_memberCount(4), '4 людини');
    expect(loc.groupDetails_memberCount(12), '12 людей');
    expect(loc.groupDetails_memberCount(31), '31 людина');
    expect(loc.groupDetails_memberCount(34), '34 людини');
  });

  test('Russian picks the one, few and many forms by the last digits', () {
    final loc = lookupAppLocalizations(const Locale('ru'));
    expect(
      loc.linkedDevicesScreen_deviceCount(0),
      'Нет привязанных устройств.',
    );
    expect(loc.linkedDevicesScreen_deviceCount(1), 'Привязано 1 устройство.');
    expect(loc.linkedDevicesScreen_deviceCount(3), 'Привязано 3 устройства.');
    expect(loc.linkedDevicesScreen_deviceCount(5), 'Привязано 5 устройств.');
    expect(loc.linkedDevicesScreen_deviceCount(11), 'Привязано 11 устройств.');
    expect(loc.linkedDevicesScreen_deviceCount(21), 'Привязано 21 устройство.');
    expect(loc.linkedDevicesScreen_deviceCount(22), 'Привязано 22 устройства.');
  });

  test('Estonian uses the singular only for exactly one', () {
    final loc = lookupAppLocalizations(const Locale('et'));
    expect(loc.linkedDevicesScreen_deviceCount(0), 'Lingitud seadmeid ei ole.');
    expect(loc.linkedDevicesScreen_deviceCount(1), 'Lingitud on 1 seade.');
    expect(loc.linkedDevicesScreen_deviceCount(5), 'Lingitud on 5 seadet.');
    expect(loc.linkedDevicesScreen_deviceCount(21), 'Lingitud on 21 seadet.');
  });

  test('Estonian agrees the device limit count with the linked count', () {
    final loc = lookupAppLocalizations(const Locale('et'));
    expect(
      loc.linkedDevicesScreen_deviceCountWithLimit(1, 5),
      'Lingitud on 1 seade 5-st.',
    );
    expect(
      loc.linkedDevicesScreen_deviceCountWithLimit(5, 5),
      'Lingitud on 5 seadet 5-st.',
    );
  });
}
