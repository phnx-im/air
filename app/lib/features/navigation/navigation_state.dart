// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

part 'navigation_state.freezed.dart';

/// State of the global app navigation.
///
/// Dart-side: the notification path only ever needed a `NotificationPolicy`,
/// so the rest moved out of Rust and a navigation change needs no bridge
/// regeneration.
@freezed
sealed class NavigationState with _$NavigationState {
  const NavigationState._();

  /// Onboarding: welcome and registration. The intro screen is always the
  /// root, [screens] stack on top of it.
  const factory NavigationState.intro({
    @Default(<IntroScreenType>[]) List<IntroScreenType> screens,
  }) = IntroState;

  const factory NavigationState.home({
    @Default(HomeNavigationState()) HomeNavigationState home,
  }) = HomeState;

  /// The chat the navigation is pointed at, open or not.
  ChatId? get chatId => switch (this) {
    HomeState(:final home) => home.chatId,
    IntroState() => null,
  };

  /// The chat that is actually showing. See [HomeNavigationState.chatOpen] for
  /// why that differs from [chatId].
  ChatId? get openChatId => switch (this) {
    HomeState(:final home) when home.chatOpen => home.chatId,
    IntroState() || HomeState() => null,
  };

  /// Whether account creation is on screen. A user appearing under the flow is
  /// not the cue to leave it: the account exists one step before it is done.
  bool get isCreatingAccount => switch (this) {
    IntroState(:final screens) =>
      screens.isNotEmpty && screens.last == IntroScreenType.accountCreation,
    HomeState() => false,
  };

  /// What the notification path should do while this state is on screen. The
  /// only part of navigation Rust reads, derived here and pushed across.
  NotificationPolicy get notificationPolicy {
    if (this case HomeState(:final home)) {
      final chatId = home.chatId;
      if (chatId != null) {
        return NotificationPolicy.suppressChat(chatId: chatId);
      }
      // A phone's chat list already shows all activity, so notifications over
      // it are noise. The desktop list sits beside a chat and suppresses none.
      if (home.activeTab == HomeTab.chats && DeviceType.isPhone) {
        return const NotificationPolicy.suppressAll();
      }
      return const NotificationPolicy.allowAll();
    }
    return const NotificationPolicy.suppressAll();
  }
}

/// Home: the main screen of the app, and everything reachable over it.
///
/// Kept simple rather than invalid-state-proof: a destination without levels
/// is an open flag, the chat details have levels and are a stack.
@freezed
abstract class HomeNavigationState with _$HomeNavigationState {
  const factory HomeNavigationState({
    /// Whether a chat is open, independently of [chatId]: a chat can close
    /// without dropping which chat it was.
    @Default(false) bool chatOpen,
    ChatId? chatId,
    @Default(HomeTab.chats) HomeTab activeTab,

    /// The open section of the profile tab. `null` is the section list, for
    /// which the two-pane layout substitutes [YouSection.profile].
    YouSection? youSection,

    /// The chat details drill-down, bottom level first. Empty means closed.
    @Default(<ChatDetailsPage>[]) List<ChatDetailsPage> chatDetails,
    @Default(false) bool createGroupOpen,
  }) = _HomeNavigationState;
}

/// One level of the chat details drill-down.
@freezed
sealed class ChatDetailsPage with _$ChatDetailsPage {
  /// The chat itself: a contact's profile or a group's details.
  const factory ChatDetailsPage.details() = DetailsPage;

  const factory ChatDetailsPage.groupMembers() = GroupMembersPage;

  const factory ChatDetailsPage.addMembers() = AddMembersPage;

  const factory ChatDetailsPage.memberDetails(UiUserId member) =
      MemberDetailsPage;

  /// The safety code for [user], whose profile is the level below this one.
  const factory ChatDetailsPage.safetyCode(UiUserId user) = SafetyCodePage;
}

/// The intro screens that stack *on top of* the root intro screen.
enum IntroScreenType {
  /// Registration, whole: the flow holds its own steps.
  accountCreation,

  linking,

  /// The developer settings before a user is loaded.
  developerSettings,
}

/// Primary destinations exposed in the mobile tab bar and desktop sidebar.
enum HomeTab { chats, profile }

/// Sections of the profile tab.
enum YouSection {
  profile,
  devices,
  account,
  preferences,
  help,

  /// Only listed while developer mode is on.
  developer,
}
