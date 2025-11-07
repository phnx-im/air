// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';

export 'package:air/core/core.dart'
    show NavigationState, IntroScreenType, DeveloperSettingsScreenType;
export 'package:air/core/core_extension.dart' show NavigationStateExtension;

class NavigationCubit implements StateStreamableSource<NavigationState> {
  NavigationCubit()
    : _impl = NavigationCubitBase(
        notificationService: DartNotificationServiceExtension.create(),
      );

  final NavigationCubitBase _impl;

  NavigationCubitBase get base => _impl;

  @override
  FutureOr<void> close() => _impl.close();

  @override
  bool get isClosed => _impl.isClosed;

  @override
  NavigationState get state => _impl.state;

  @override
  Stream<NavigationState> get stream => _impl.stream();

  // Methods

  Future<void> closeChat() => _impl.closeChat();

  Future<void> openChat(ChatId chatId) => _impl.openChat(chatId: chatId);

  Future<void> openChatDetails() => _impl.openChatDetails();

  Future<void> openAddMembers() => _impl.openAddMembers();

  Future<void> openCreateGroup() => _impl.openCreateGroup();

  Future<void> openGroupMembers() => _impl.openGroupMembers();

  Future<void> openMemberDetails(UiUserId member) =>
      _impl.openMemberDetails(member: member);

  Future<void> openDeveloperSettings({
    DeveloperSettingsScreenType screen = DeveloperSettingsScreenType.root,
  }) => _impl.openDeveloperSettings(screen: screen);

  Future<void> openHome() => _impl.openHome();

  Future<void> openIntro() => _impl.openInto();

  Future<void> openIntroScreen(IntroScreenType screen) =>
      _impl.openIntroScreen(screen: screen);

  Future<void> openUserSettings({
    UserSettingsScreenType screen = UserSettingsScreenType.root,
  }) => _impl.openUserSettings(screen: screen);

  bool pop() => _impl.pop();

  Future<void> openServerChoice() =>
      _impl.openIntroScreen(screen: const IntroScreenType.signUp());
}
