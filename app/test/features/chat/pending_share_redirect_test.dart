// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/share/pending_share.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';
import '../chat_list/chat_list_content_test.dart';

class MockNotificationContext extends Mock implements NotificationContextBase {}

const share = PendingShare(text: 'hi');

void main() {
  group('PendingShareRedirect', () {
    late NavigationCubit navigationCubit;

    // The chat list fixtures: a contact the reader can share into, a blocked
    // contact, and an id no chat has.
    final shareable = chats[0];
    final blocked = chats[4];
    final missingId = 99.chatId();

    setUpAll(() {
      registerFallbackValue(const NotificationPolicy.suppressAll());
      registerFallbackValue(missingId);
    });

    setUp(() {
      final notificationContext = MockNotificationContext();
      when(
        () => notificationContext.chatOpened(chatId: any(named: 'chatId')),
      ).thenAnswer((_) async {});
      navigationCubit = NavigationCubit(
        notificationContext: notificationContext,
      );
      navigationCubit.openHome();
    });

    tearDown(() => navigationCubit.close());

    HomeNavigationState home() => (navigationCubit.state as HomeState).home;

    Widget buildSubject({
      required ChatId chatId,
      FakeChatsRepository? repository,
    }) => RepositoryProvider<ChatsRepository>.value(
      value: repository ?? FakeChatsRepository([shareable, blocked]),
      child: BlocProvider<NavigationCubit>.value(
        value: navigationCubit,
        child: PendingShareRedirect(
          chatId: chatId,
          child: const SizedBox.shrink(),
        ),
      ),
    );

    testWidgets('sends a share addressed to a missing chat to the picker', (
      tester,
    ) async {
      await navigationCubit.openShare(share, chatId: missingId);

      await tester.pumpWidget(buildSubject(chatId: missingId));
      await tester.pumpAndSettle();

      expect(
        home(),
        const HomeNavigationState(
          pendingShare: share,
          shareDestinationOpen: true,
        ),
      );
    });

    testWidgets('sends a share addressed to a blocked chat to the picker', (
      tester,
    ) async {
      await navigationCubit.openShare(share, chatId: blocked.id);

      await tester.pumpWidget(buildSubject(chatId: blocked.id));
      await tester.pumpAndSettle();

      expect(home().shareDestinationOpen, isTrue);
      expect(home().pendingShare, share);
    });

    testWidgets('leaves a share addressed to a shareable chat alone', (
      tester,
    ) async {
      await navigationCubit.openShare(share, chatId: shareable.id);

      await tester.pumpWidget(buildSubject(chatId: shareable.id));
      await tester.pumpAndSettle();

      expect(
        home(),
        HomeNavigationState(
          chatOpen: true,
          chatId: shareable.id,
          pendingShare: share,
        ),
      );
    });

    testWidgets('ignores a share addressed to another chat', (tester) async {
      await navigationCubit.openShare(share, chatId: missingId);

      // Mounted for a chat navigation is not pointed at.
      await tester.pumpWidget(buildSubject(chatId: shareable.id));
      await tester.pumpAndSettle();

      expect(home().shareDestinationOpen, isFalse);
      expect(home().chatId, missingId);
    });

    // On cold start the chat may be on its way rather than gone.
    testWidgets('waits for the repository to load before judging', (
      tester,
    ) async {
      final repository = FakeChatsRepository([shareable], loaded: false);
      await navigationCubit.openShare(share, chatId: missingId);

      await tester.pumpWidget(
        buildSubject(chatId: missingId, repository: repository),
      );
      await tester.pumpAndSettle();
      expect(home().shareDestinationOpen, isFalse);

      repository.load();
      await tester.pumpAndSettle();

      expect(home().shareDestinationOpen, isTrue);
      expect(home().pendingShare, share);
    });

    testWidgets('redirects when the open chat is deleted', (tester) async {
      final repository = FakeChatsRepository([shareable]);
      await navigationCubit.openShare(share, chatId: shareable.id);

      await tester.pumpWidget(
        buildSubject(chatId: shareable.id, repository: repository),
      );
      await tester.pumpAndSettle();
      expect(home().shareDestinationOpen, isFalse);

      repository.remove(shareable.id);
      await tester.pumpAndSettle();

      expect(home().shareDestinationOpen, isTrue);
      expect(home().pendingShare, share);
    });

    testWidgets('redirects a share that arrives for the open chat', (
      tester,
    ) async {
      await navigationCubit.openChat(missingId);
      await tester.pumpWidget(buildSubject(chatId: missingId));
      await tester.pumpAndSettle();
      expect(home().shareDestinationOpen, isFalse);

      await navigationCubit.openShare(share, chatId: missingId);
      await tester.pumpAndSettle();

      expect(home().shareDestinationOpen, isTrue);
      expect(home().pendingShare, share);
    });
  });
}
