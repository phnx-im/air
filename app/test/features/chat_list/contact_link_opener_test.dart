// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/chat_list/contact_link_opener.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../mocks.dart';

void main() {
  group('ContactLinkOpener', () {
    const alice = UiUsername(plaintext: 'alice');
    const bob = UiUsername(plaintext: 'bob');
    const signUp = NavigationState.intro(
      screens: [IntroScreenType.accountCreation],
    );
    const home = NavigationState.home();

    late StreamController<NavigationState> navigation;
    late List<UiUsername> shown;
    late ContactLinkOpener opener;

    void start(NavigationState initialState) {
      final navigationCubit = MockNavigationCubit();
      whenListen(
        navigationCubit,
        navigation.stream,
        initialState: initialState,
      );
      opener = ContactLinkOpener(
        navigationCubit: navigationCubit,
        show: shown.add,
      );
    }

    Future<void> navigate(NavigationState state) async {
      navigation.add(state);
      await pumpEventQueue();
    }

    setUp(() {
      navigation = StreamController<NavigationState>.broadcast();
      shown = [];
    });

    tearDown(() async {
      await opener.dispose();
      await navigation.close();
    });

    test('opens a link on home right away', () {
      start(home);

      opener.open(alice);

      expect(shown, [alice]);
    });

    test('opens a link from sign-up once home opens', () async {
      start(signUp);

      opener.open(alice);
      expect(shown, isEmpty);

      await navigate(home);
      expect(shown, [alice]);
    });

    test('keeps a waiting link through other intro screens', () async {
      start(const NavigationState.intro());

      opener.open(alice);
      await navigate(signUp);
      expect(shown, isEmpty);

      await navigate(home);
      expect(shown, [alice]);
    });

    test('opens only the latest waiting link', () async {
      start(signUp);

      opener.open(alice);
      opener.open(bob);
      await navigate(home);

      expect(shown, [bob]);
    });

    test('opens a waiting link once', () async {
      start(signUp);

      opener.open(alice);
      await navigate(home);
      await navigate(const NavigationState.home(home: HomeNavigationState()));
      await navigate(signUp);
      await navigate(home);

      expect(shown, [alice]);
    });
  });
}
