// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:uuid/uuid.dart';

class MockNotificationContext extends Mock implements NotificationContextBase {}

final _chatId = ChatId(uuid: const Uuid().v4obj());
final _otherChatId = ChatId(uuid: const Uuid().v4obj());
final _member = UiUserId(uuid: const Uuid().v4obj(), domain: 'example.com');

/// The home state a cubit is in once it is driven to [home].
HomeNavigationState _home(NavigationCubit cubit) =>
    (cubit.state as HomeState).home;

void main() {
  late MockNotificationContext notificationContext;

  setUpAll(() {
    registerFallbackValue(const NotificationPolicy.suppressAll());
    registerFallbackValue(_chatId);
  });

  setUp(() {
    notificationContext = MockNotificationContext();
    when(
      () => notificationContext.chatOpened(chatId: any(named: 'chatId')),
    ).thenAnswer((_) async {});
  });

  NavigationCubit subject() =>
      NavigationCubit(notificationContext: notificationContext);

  group('NavigationCubit', () {
    test('starts on the intro with no screens over it', () {
      expect(subject().state, const NavigationState.intro());
    });

    test('stacks intro screens, and refuses to repeat the topmost', () {
      final cubit = subject();

      cubit.openLinking();
      cubit.openSignUp();
      cubit.openSignUp();

      expect(
        cubit.state,
        const NavigationState.intro(
          screens: [IntroScreenType.linking, IntroScreenType.accountCreation],
        ),
      );
    });

    test('account creation is what the intro reports it is showing', () {
      final cubit = subject();
      expect(cubit.state.isCreatingAccount, isFalse);

      cubit.openSignUp();
      expect(cubit.state.isCreatingAccount, isTrue);

      cubit.openHome();
      expect(cubit.state.isCreatingAccount, isFalse);
    });

    test('opening a chat drops what was open over the previous one', () async {
      final cubit = subject();
      cubit.openHome();
      await cubit.openChat(_chatId);
      cubit.openChatDetails();
      cubit.openGroupMembers();

      await cubit.openChat(_otherChatId);

      expect(
        _home(cubit),
        HomeNavigationState(chatOpen: true, chatId: _otherChatId),
      );
    });

    test('opening a chat clears the notifications it already posted', () async {
      final cubit = subject();
      cubit.openHome();

      await cubit.openChat(_chatId);

      verify(() => notificationContext.chatOpened(chatId: _chatId)).called(1);
    });

    test('closing a chat leaves the tab and the open section alone', () async {
      final cubit = subject();
      cubit.openHome();
      await cubit.openChat(_chatId);
      cubit.openChatDetails();
      cubit.openYouSection(YouSection.help);

      cubit.closeChat();

      expect(
        _home(cubit),
        const HomeNavigationState(youSection: YouSection.help),
      );
    });

    test('switching tab drops the open section', () {
      final cubit = subject();
      cubit.openHome();
      cubit.openYouSection(YouSection.help);

      cubit.switchTab(HomeTab.profile);

      expect(
        _home(cubit),
        const HomeNavigationState(activeTab: HomeTab.profile),
      );
    });

    test('the developer settings take the tab that hosts them along', () {
      final cubit = subject();
      cubit.openHome();

      cubit.openDeveloperSettings();

      expect(
        _home(cubit),
        const HomeNavigationState(
          activeTab: HomeTab.profile,
          youSection: YouSection.developer,
        ),
      );
    });

    test('before a user is loaded they are a screen of their own', () {
      final cubit = subject();

      cubit.openDeveloperSettings();

      expect(
        cubit.state,
        const NavigationState.intro(
          screens: [IntroScreenType.developerSettings],
        ),
      );
    });

    test('the intro leaves nothing to pop once its screens are gone', () {
      final cubit = subject();
      cubit.openSignUp();

      expect(cubit.pop(), isTrue);
      expect(cubit.pop(), isFalse);
      expect(cubit.state, const NavigationState.intro());
    });
  });

  // The chat details record the order their levels were opened in, so back
  // unwinds them in that order. Everything else records only that it is open,
  // and closes in the order the pop logic tests for it.
  group('NavigationCubit stack order', () {
    test('unwinds the chat details drill-down one level at a time', () async {
      final cubit = subject();
      cubit.openHome();
      await cubit.openChat(_chatId);
      cubit.openChatDetails();
      cubit.openGroupMembers();
      cubit.openMemberDetails(_member);
      cubit.openSafetyCode(_member);

      expect(_home(cubit).chatDetails, [
        const ChatDetailsPage.details(),
        const ChatDetailsPage.groupMembers(),
        ChatDetailsPage.memberDetails(_member),
        ChatDetailsPage.safetyCode(_member),
      ]);

      expect(cubit.pop(), isTrue);
      expect(
        _home(cubit).chatDetails.last,
        ChatDetailsPage.memberDetails(_member),
      );

      expect(cubit.pop(), isTrue);
      expect(
        _home(cubit).chatDetails.last,
        const ChatDetailsPage.groupMembers(),
      );

      expect(cubit.pop(), isTrue);
      expect(_home(cubit).chatDetails, [const ChatDetailsPage.details()]);

      expect(cubit.pop(), isTrue);
      expect(_home(cubit).chatDetails, isEmpty);
      expect(_home(cubit).chatOpen, isTrue);

      expect(cubit.pop(), isTrue);
      expect(_home(cubit).chatOpen, isFalse);

      // The chat id outlives the open chat, so there is nothing left to close.
      expect(cubit.pop(), isFalse);
    });

    // The flags this replaced could not tell the two apart: they recorded that
    // a member's profile was open, not what it was opened from.
    test('takes a member back to the level they were reached from', () async {
      final cubit = subject();
      cubit.openHome();
      await cubit.openChat(_chatId);

      // From the group's preview of its members.
      cubit.openChatDetails();
      cubit.openMemberDetails(_member);
      expect(cubit.pop(), isTrue);
      expect(_home(cubit).chatDetails, [const ChatDetailsPage.details()]);

      // From the full member list.
      cubit.openGroupMembers();
      cubit.openMemberDetails(_member);
      expect(cubit.pop(), isTrue);
      expect(_home(cubit).chatDetails, [
        const ChatDetailsPage.details(),
        const ChatDetailsPage.groupMembers(),
      ]);
    });

    test('opens one level for a control tapped twice', () async {
      final cubit = subject();
      cubit.openHome();
      await cubit.openChat(_chatId);

      cubit.openChatDetails();
      cubit.openChatDetails();

      expect(_home(cubit).chatDetails, [const ChatDetailsPage.details()]);
    });

    test('closing the details closes every level of them', () async {
      final cubit = subject();
      cubit.openHome();
      await cubit.openChat(_chatId);
      cubit.openChatDetails();
      cubit.openGroupMembers();
      cubit.openMemberDetails(_member);

      cubit.closeChatDetails();

      expect(
        _home(cubit),
        HomeNavigationState(chatOpen: true, chatId: _chatId),
      );
    });

    test('an open section outranks everything below it', () async {
      final cubit = subject();
      cubit.openHome();
      await cubit.openChat(_chatId);
      cubit.openChatDetails();
      cubit.openYouSection(YouSection.help);

      expect(cubit.pop(), isTrue);

      expect(_home(cubit).youSection, isNull);
      expect(_home(cubit).chatDetails, [const ChatDetailsPage.details()]);
    });
  });

  // Rust reads nothing else of the navigation state, so this mapping is the
  // whole contract between the two.
  group('NavigationCubit notification policy', () {
    setUp(() => debugDefaultTargetPlatformOverride = TargetPlatform.iOS);
    tearDown(() => debugDefaultTargetPlatformOverride = null);

    NotificationPolicy policyAfter(void Function(NavigationCubit) drive) {
      final cubit = subject();
      drive(cubit);
      final captured = verify(
        () =>
            notificationContext.setPolicy(policy: captureAny(named: 'policy')),
      ).captured;
      return captured.last as NotificationPolicy;
    }

    test('suppresses everything before a user is loaded', () {
      expect(
        policyAfter((cubit) => cubit.openSignUp()),
        const NotificationPolicy.suppressAll(),
      );
    });

    test('suppresses the chat that is on screen', () {
      expect(
        policyAfter((cubit) {
          cubit.openHome();
          cubit.openChat(_chatId);
        }),
        NotificationPolicy.suppressChat(chatId: _chatId),
      );
    });

    test("keeps suppressing it while a modal covers it", () {
      expect(
        policyAfter((cubit) {
          cubit.openHome();
          cubit.openChat(_chatId);
          cubit.openChatDetails();
        }),
        NotificationPolicy.suppressChat(chatId: _chatId),
      );
    });

    test("suppresses everything on a phone's chat list", () {
      expect(
        policyAfter((cubit) {
          cubit.openHome();
          cubit.switchTab(HomeTab.profile);
          cubit.switchTab(HomeTab.chats);
        }),
        const NotificationPolicy.suppressAll(),
      );
    });

    test('allows everything away from the chats tab', () {
      expect(
        policyAfter((cubit) {
          cubit.openHome();
          cubit.switchTab(HomeTab.profile);
        }),
        const NotificationPolicy.allowAll(),
      );
    });

    test('allows everything on a desktop chat list, which stands in for no '
        'chat', () {
      debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
      expect(
        policyAfter((cubit) {
          cubit.openHome();
          cubit.switchTab(HomeTab.profile);
          cubit.switchTab(HomeTab.chats);
        }),
        const NotificationPolicy.allowAll(),
      );
    });
  });
}
