// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('UserSettingsCubit', () {
    late UserSettingsCubit cubit;

    setUp(() => cubit = UserSettingsCubit());
    tearDown(() => cubit.close());

    // The developer section is reachable before login, where there is no user
    // to persist to, so the value is kept in memory until one arrives.
    test('keeps a setting made before login', () async {
      await cubit.setInterfaceScale(value: 1.5);
      await cubit.setDeveloperMode(value: true);

      expect(cubit.state.interfaceScale, 1.5);
      expect(cubit.state.developerMode, isTrue);
    });

    test('reports a setting made before login to its listeners', () {
      expectLater(
        cubit.stream,
        emitsInOrder([
          const UserSettings(interfaceScale: 1.5),
          const UserSettings(interfaceScale: 1.5, developerMode: true),
        ]),
      );

      cubit.setInterfaceScale(value: 1.5);
      cubit.setDeveloperMode(value: true);
    });

    // Logging out drops the user the settings belonged to, so the next login
    // starts from the defaults.
    test('forgets the settings on detach', () async {
      await cubit.setInterfaceScale(value: 1.5);

      cubit.detach();

      expect(cubit.state, const UserSettings());
    });

    test('experimental features follow developer mode', () {
      expect(
        const UserSettings(
          experimentalFeatures: true,
        ).experimentalFeaturesActive,
        isFalse,
      );
      expect(
        const UserSettings(
          developerMode: true,
          experimentalFeatures: true,
        ).experimentalFeaturesActive,
        isTrue,
      );
    });
  });
}
