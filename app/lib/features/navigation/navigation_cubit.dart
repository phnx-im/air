// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_state.dart';
import 'package:air/share/pending_share.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

export 'package:air/features/navigation/navigation_state.dart';

/// Provides the navigation state and actions to the app. `AppRouter`
/// translates the state into screens.
///
/// Rust holds no navigation state: it only needs the [NotificationPolicy]
/// this pushes across on every change.
class NavigationCubit extends Cubit<NavigationState> {
  NavigationCubit({required this.notificationContext})
    : super(const NavigationState.intro());

  /// The bridge the notification policy is pushed through.
  final NotificationContextBase notificationContext;

  /// Flagged when a shared payload has been handed to over to e.g. the message
  /// composer.
  bool _shareHandedOver = false;

  /// Keeps the policy Rust reads in step with what is on screen. The intro's
  /// policy is Rust's default, so there is nothing to push initially.
  @override
  void onChange(Change<NavigationState> change) {
    super.onChange(change);
    notificationContext.setPolicy(policy: change.nextState.notificationPolicy);

    // A share's extracted files live until they're taken.
    final dropped = change.currentState.pendingShare;
    if (!_shareHandedOver &&
        dropped != null &&
        dropped != change.nextState.pendingShare) {
      unawaited(dropped.deleteFiles());
    }
  }

  // Navigation actions

  void openIntro() => emit(const NavigationState.intro());

  void openHome() => emit(const NavigationState.home());

  /// Opens [chatId], closing everything open over the previous chat, and
  /// clears the chat's OS notifications. A pending share goes with it, so
  /// reaching a chat any other way than through the picker drops the share.
  Future<void> openChat(ChatId chatId) => _openChat(chatId);

  /// Closes the chat and everything reachable over it.
  void closeChat() => _updateHome(
    (home) => home.copyWith(
      chatOpen: false,
      chatId: null,
      chatDetails: const [],
      createGroupOpen: false,
      pendingShare: null,
    ),
  );

  // Android share handoff

  /// Routes content the share activity handed over.
  Future<void> openShare(PendingShare share, {ChatId? chatId}) async {
    if (chatId == null) {
      emit(
        NavigationState.home(
          home: HomeNavigationState(
            pendingShare: share,
            shareDestinationOpen: true,
          ),
        ),
      );
    } else {
      await _openChat(chatId, share: share);
    }
  }

  /// Opens the chat the user picked for the pending share.
  Future<void> openShareDestination(ChatId chatId) async {
    if (state case HomeState(home: HomeNavigationState(:final pendingShare?))) {
      await _openChat(chatId, share: pendingShare);
    }
  }

  /// The composer took the share and owns its files from here on.
  void shareConsumed() {
    if (state case HomeState(:final home) when home.pendingShare != null) {
      _shareHandedOver = true;
      emit(NavigationState.home(home: home.copyWith(pendingShare: null)));
      _shareHandedOver = false;
    }
  }

  /// Drops the share without staging it, from the picker.
  void cancelShare() => _updateHome(
    (home) => home.copyWith(pendingShare: null, shareDestinationOpen: false),
  );

  void openChatDetails() => _pushChatDetails(const ChatDetailsPage.details());

  void openSafetyCode(UiUserId user) =>
      _pushChatDetails(ChatDetailsPage.safetyCode(user));

  void openAddMembers() => _pushChatDetails(const ChatDetailsPage.addMembers());

  void openGroupMembers() =>
      _pushChatDetails(const ChatDetailsPage.groupMembers());

  void openCreateGroup() =>
      _updateHome((home) => home.copyWith(createGroupOpen: true));

  /// Opens a member's profile over the current level, or as the drill-down's
  /// bottom level when reached from a message.
  void openMemberDetails(UiUserId member) =>
      _pushChatDetails(ChatDetailsPage.memberDetails(member));

  /// Closes the chat details whole, from whichever level is showing.
  void closeChatDetails() =>
      _updateHome((home) => home.copyWith(chatDetails: const []));

  /// Switches tab. A tab always lands on its root, even the current one.
  void switchTab(HomeTab tab) =>
      _updateHome((home) => home.copyWith(activeTab: tab, youSection: null));

  void openYouSection(YouSection section) =>
      _updateHome((home) => home.copyWith(youSection: section));

  void closeYouSection() =>
      _updateHome((home) => home.copyWith(youSection: null));

  /// Opens the developer settings: a profile tab section once a user is
  /// loaded, a screen of its own before that.
  void openDeveloperSettings() {
    switch (state) {
      case IntroState():
        _pushIntroScreen(IntroScreenType.developerSettings);
      // Reached from other tabs too, so it switches the tab along.
      case HomeState(:final home):
        emit(
          NavigationState.home(
            home: home.copyWith(
              activeTab: HomeTab.profile,
              youSection: YouSection.developer,
            ),
          ),
        );
    }
  }

  void openLinking() => _pushIntroScreen(IntroScreenType.linking);

  void openSignUp() => _pushIntroScreen(IntroScreenType.accountCreation);

  /// Closes the topmost destination, reporting whether there was one to close.
  bool pop() {
    switch (state) {
      case IntroState(:final screens):
        if (screens.isEmpty) return false;
        emit(
          NavigationState.intro(
            screens: screens.sublist(0, screens.length - 1),
          ),
        );
        return true;
      case HomeState(:final home):
        final next = _popHome(home);
        if (next == null) return false;
        emit(NavigationState.home(home: next));
        return true;
    }
  }

  /// The home state one level up, or `null` at the root.
  static HomeNavigationState? _popHome(HomeNavigationState home) {
    if (home.shareDestinationOpen) {
      return home.copyWith(pendingShare: null, shareDestinationOpen: false);
    }
    if (home.youSection != null) return home.copyWith(youSection: null);
    if (home.activeTab != HomeTab.chats) {
      return home.copyWith(activeTab: HomeTab.chats);
    }
    if (home.chatDetails.isNotEmpty) {
      return home.copyWith(
        chatDetails: home.chatDetails.sublist(0, home.chatDetails.length - 1),
      );
    }
    if (home.createGroupOpen) return home.copyWith(createGroupOpen: false);
    // (Possibly) drop the share when navigating back.
    if (home.chatOpen) {
      return home.copyWith(chatOpen: false, pendingShare: null);
    }
    return null;
  }

  /// Emits [chatId] as the open chat, with an optional pending share.
  Future<void> _openChat(ChatId chatId, {PendingShare? share}) async {
    emit(
      NavigationState.home(
        home: HomeNavigationState(
          chatOpen: true,
          chatId: chatId,
          pendingShare: share,
        ),
      ),
    );
    await notificationContext.chatOpened(chatId: chatId);
  }

  void _pushChatDetails(ChatDetailsPage page) => _updateHome(
    (home) => home.chatDetails.lastOrNull == page
        ? home
        : home.copyWith(chatDetails: [...home.chatDetails, page]),
  );

  /// Applies [update] to the home state, leaving the intro alone.
  void _updateHome(
    HomeNavigationState Function(HomeNavigationState home) update,
  ) {
    if (state case HomeState(:final home)) {
      emit(NavigationState.home(home: update(home)));
    }
  }

  /// Pushes an intro screen, unless it is already the topmost one.
  void _pushIntroScreen(IntroScreenType screen) {
    if (state case IntroState(
      :final screens,
    ) when screens.lastOrNull != screen) {
      emit(NavigationState.intro(screens: [...screens, screen]));
    }
  }
}
