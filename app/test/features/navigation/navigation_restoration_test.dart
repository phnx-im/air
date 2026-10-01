// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/navigation/navigation_restoration.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:uuid/uuid.dart';

class MockNotificationContext extends Mock implements NotificationContextBase {}

/// Stands in for the engine: captures what the framework sends and feeds it
/// back as the data of a restarted process.
class _FakeRestorationManager extends RestorationManager {
  Uint8List? _sent;

  @override
  void initChannels() {}

  @override
  Future<void> sendToEngine(Uint8List encodedData) async => _sent = encodedData;

  /// Delivers a (new) root bucket built from [data], as the engine does at
  /// startup and on a root swap.
  void deliver(Uint8List? data) {
    // Like the engine, keep the delivered data until the framework sends new.
    _sent = data;
    handleRestorationUpdateFromEngine(enabled: true, data: data);
  }

  /// What Android would keep if the process were killed now.
  Uint8List? kill() {
    flushData();
    return _sent;
  }
}

final _chatId = ChatId(uuid: const Uuid().v4obj());

const _savedHome = HomeNavigationState(activeTab: HomeTab.profile);

Future<void> _settle() => Future<void>.delayed(Duration.zero);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() {
    registerFallbackValue(const NotificationPolicy.suppressAll());
    registerFallbackValue(_chatId);
  });

  setUp(() {
    // A root bucket requested before [deliver] waits on the engine forever.
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          SystemChannels.restoration,
          (_) => Completer<Object?>().future,
        );
  });

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(SystemChannels.restoration, null);
  });

  /// Starts a process on [data] and returns its restoration once the bucket is
  /// claimed.
  Future<(NavigationRestoration, _FakeRestorationManager)> start(
    Uint8List? data,
  ) async {
    final manager = _FakeRestorationManager()..deliver(data);
    final restoration = NavigationRestoration(manager);
    addTearDown(restoration.dispose);
    await _settle();
    return (restoration, manager);
  }

  /// Runs a process that ends on [state] and returns what it left behind.
  Future<Uint8List?> runUntilKilled(NavigationState state) async {
    final (restoration, manager) = await start(null);
    restoration.persist(state);
    return manager.kill();
  }

  group('NavigationRestoration', () {
    test(
      'restores the tab and the open chat of the previous process',
      () async {
        final data = await runUntilKilled(
          NavigationState.home(
            home: HomeNavigationState(
              activeTab: HomeTab.profile,
              chatId: _chatId,
              chatOpen: true,
              chatDetails: const [ChatDetailsPage.details()],
            ),
          ),
        );

        final (restoration, _) = await start(data);

        expect(
          restoration.takeRestoredHome(),
          HomeNavigationState(
            activeTab: HomeTab.profile,
            chatId: _chatId,
            chatOpen: true,
          ),
        );
      },
    );

    test('hands the restored home out only once', () async {
      final data = await runUntilKilled(
        const NavigationState.home(home: _savedHome),
      );

      final (restoration, _) = await start(data);

      expect(restoration.takeRestoredHome(), _savedHome);
      expect(restoration.takeRestoredHome(), isNull);
    });

    test('keeps the saved home until the next change', () async {
      final data = await runUntilKilled(
        const NavigationState.home(home: _savedHome),
      );

      // Built before the data arrives, as at app startup.
      final manager = _FakeRestorationManager();
      final restoration = NavigationRestoration(manager);
      addTearDown(restoration.dispose);
      manager.deliver(data);
      await _settle();

      final (next, _) = await start(manager.kill());
      expect(next.takeRestoredHome(), _savedHome);
    });

    test('restores nothing on a fresh start', () async {
      final (restoration, _) = await start(null);

      expect(restoration.takeRestoredHome(), isNull);
    });

    test('intro drops the pending restore and the saved home', () async {
      final data = await runUntilKilled(
        const NavigationState.home(home: _savedHome),
      );

      final (restoration, manager) = await start(data);
      restoration.persist(const NavigationState.intro());
      expect(restoration.takeRestoredHome(), isNull);

      final (next, _) = await start(manager.kill());
      expect(next.takeRestoredHome(), isNull);
    });

    test('writes a state reported before the bucket arrived', () async {
      final manager = _FakeRestorationManager();
      final restoration = NavigationRestoration(manager);
      addTearDown(restoration.dispose);

      restoration.persist(const NavigationState.home(home: _savedHome));
      manager.deliver(null);
      await _settle();

      final (next, _) = await start(manager.kill());
      expect(next.takeRestoredHome(), _savedHome);
    });

    test('carries the current state over a root bucket swap', () async {
      final (restoration, manager) = await start(null);
      restoration.persist(const NavigationState.home(home: _savedHome));

      manager.deliver(null);
      await _settle();

      final (next, _) = await start(manager.kill());
      expect(next.takeRestoredHome(), _savedHome);
    });

    test('does not write after dispose while a swap is in flight', () async {
      final manager = _FakeRestorationManager()..deliver(null);
      final restoration = NavigationRestoration(manager);
      await _settle();
      restoration.persist(const NavigationState.home(home: _savedHome));

      manager.deliver(null);
      restoration.dispose();
      await _settle();

      final (next, _) = await start(manager.kill());
      expect(next.takeRestoredHome(), isNull);
    });

    test('ignores saved data it cannot read', () async {
      final manager = _FakeRestorationManager()..deliver(null);
      final root = await manager.rootBucket;
      root!
          .claimChild(NavigationRestoration.bucketId, debugOwner: null)
          .write(NavigationRestoration.lastHomeKey, 'x');

      final (restoration, _) = await start(manager.kill());

      expect(restoration.takeRestoredHome(), isNull);
    });

    test('falls back to the chats tab for an unknown tab', () async {
      final manager = _FakeRestorationManager()..deliver(null);
      final root = await manager.rootBucket;
      root!.claimChild(NavigationRestoration.bucketId, debugOwner: null).write(
        NavigationRestoration.lastHomeKey,
        {'tab': 'gone', 'chatOpen': false},
      );

      final (restoration, _) = await start(manager.kill());

      expect(restoration.takeRestoredHome(), const HomeNavigationState());
    });
  });

  test(
    'NavigationCubit persists its changes through the restoration',
    () async {
      final notificationContext = MockNotificationContext();
      when(() => notificationContext.chatOpened(chatId: any(named: 'chatId')))
          .thenAnswer((_) async {});
      final manager = _FakeRestorationManager()..deliver(null);
      final cubit = NavigationCubit(
        notificationContext: notificationContext,
        restoration: NavigationRestoration(manager),
      );
      addTearDown(cubit.close);
      await _settle();

      cubit.openHome();
      await cubit.openChat(_chatId);

      final (restoration, _) = await start(manager.kill());
      expect(
        restoration.takeRestoredHome(),
        HomeNavigationState(chatId: _chatId, chatOpen: true),
      );
    },
  );
}
